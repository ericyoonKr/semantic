import importlib
import json
import os
import re
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import sysconfig
import time
import urllib.error
from pathlib import Path
from urllib.request import Request, urlopen

GEMMA_REPO = os.environ.get("GEMMA_MODEL", "google/gemma-4-26b-a4b-it")
GEMMA_NAME = "gemma4"
BGE_REPO = "BAAI/bge-m3"
BGE_NAME = "BAAI/bge-m3"
BGE_PORT = 8001
GEMMA_PORT = 8002
BGE_MEM = 0.08
GEMMA_MEM = 0.85
GEMMA_MAX_LEN = int(os.environ.get("GEMMA_MAX_LEN", "32768"))
GEMMA_MAX_SEQS = 16
GEMMA_BATCHED_TOKENS = 8192

WORK = Path.cwd()
BIN = WORK / "bin"
CLOUDFLARED = BIN / "cloudflared"
CF_URL = "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-amd64"

KEY = os.environ.get("MODEL_API_KEY") or secrets.token_urlsafe(32)
KEY_NEW = "MODEL_API_KEY" not in os.environ
AUTH = {"Authorization": "Bearer " + KEY}


def colab_secret(name):
    try:
        from google.colab import userdata
        return userdata.get(name)
    except Exception:
        return None


HF_TOKEN = os.environ.get("HF_TOKEN") or colab_secret("HF_TOKEN")
if HF_TOKEN:
    os.environ["HF_TOKEN"] = HF_TOKEN


def tail(path, size=12000):
    p = Path(path)
    if not p.exists():
        return "(로그 없음)"
    with p.open("rb") as f:
        f.seek(0, os.SEEK_END)
        f.seek(max(0, f.tell() - size))
        return f.read().decode("utf-8", errors="replace")


def http_json(url, body=None, timeout=10):
    data = None if body is None else json.dumps(body, ensure_ascii=False).encode("utf-8")
    headers = dict(AUTH)
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = Request(url, data=data, headers=headers, method="GET" if body is None else "POST")
    with urlopen(req, timeout=timeout) as r:
        return json.load(r)


def served(port):
    try:
        models = http_json(f"http://127.0.0.1:{port}/v1/models", timeout=5).get("data", [])
        return models[0].get("id") if models else None
    except Exception:
        return None


def port_open(port):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1):
            return True
    except OSError:
        return False


def stop(proc):
    if proc is None or proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=10)
    except Exception:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except Exception:
            pass


def gpu():
    smi = shutil.which("nvidia-smi")
    if not smi:
        raise RuntimeError("GPU를 찾을 수 없습니다. 런타임 유형을 GPU(G4)로 바꾸십시오.")
    out = subprocess.run([smi, "--query-gpu=name,memory.total", "--format=csv,noheader"], capture_output=True, text=True)
    print("GPU:", out.stdout.strip())
    total_mib = int(re.findall(r"(\d+)\s*MiB", out.stdout)[0]) if "MiB" in out.stdout else 0
    if total_mib and total_mib < 80000:
        raise RuntimeError("GPU 메모리가 80GB보다 작습니다. Gemma 4 26B bf16(약 52GB)과 KV 캐시가 들어가지 않습니다.")


def find_vllm():
    found = shutil.which("vllm")
    if found:
        return found
    cand = Path(sysconfig.get_path("scripts")) / "vllm"
    return str(cand) if cand.is_file() else None


def ensure_vllm():
    vllm = find_vllm()
    if not vllm:
        print("vLLM을 설치합니다. 수 분 걸릴 수 있습니다.")
        subprocess.run([sys.executable, "-m", "pip", "install", "--upgrade", "vllm"], check=True)
        importlib.invalidate_caches()
        vllm = find_vllm()
    if not vllm:
        raise RuntimeError("vLLM 실행 파일을 찾을 수 없습니다.")
    probe = subprocess.run([sys.executable, "-c", "import torchaudio"], capture_output=True, text=True)
    if probe.returncode != 0 and "different CUDA versions" in (probe.stdout + probe.stderr):
        subprocess.run([sys.executable, "-m", "pip", "uninstall", "-y", "torchaudio"], check=False)
    ver = subprocess.run([sys.executable, "-c", "import vllm;print(vllm.__version__)"], capture_output=True, text=True)
    print("vLLM 판:", ver.stdout.strip() or ver.stderr.strip()[-300:])
    return vllm


def check_repo():
    from huggingface_hub import HfApi
    api = HfApi(token=HF_TOKEN)
    try:
        api.model_info(GEMMA_REPO)
        print("Gemma 저장소 확인:", GEMMA_REPO)
    except Exception as e:
        names = [m.id for m in api.list_models(author="google", search="gemma-4", limit=50)]
        print("찾지 못함:", GEMMA_REPO, "-", type(e).__name__)
        print("google의 gemma-4 저장소:", names)
        raise RuntimeError("GEMMA_MODEL 환경변수에 위 목록의 정확한 이름을 넣고 다시 실행하십시오. 승인이 필요한 모델이면 HF_TOKEN(Colab 보안 비밀)을 넣으십시오.")


def parser_names(kind):
    code = {
        "tool": [
            "from vllm.entrypoints.openai.tool_parsers import ToolParserManager as M",
            "from vllm.tool_parsers import ToolParserManager as M",
        ],
        "reasoning": [
            "from vllm.reasoning import ReasoningParserManager as M",
        ],
    }[kind]
    for line in code:
        script = line + "\nprint(','.join(sorted(getattr(M, 'tool_parsers', None) or getattr(M, 'reasoning_parsers', None) or getattr(M, 'lazy_parsers', {}))))"
        r = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            return r.stdout.strip().split(",")
    return []


def pick(names, wanted):
    for w in wanted:
        if w in names:
            return w
    return None


def start(vllm, args, port, label, minutes=25):
    if served(port):
        print(label, "이미 떠 있음:", served(port))
        return None
    if port_open(port):
        raise RuntimeError(f"{port}번 포트가 이미 쓰이고 있습니다.")
    log = WORK / f"vllm_{label}.log"
    cmd = [vllm, "serve", *args, "--host", "127.0.0.1", "--port", str(port), "--api-key", KEY]
    print(label, "시작:", " ".join(a if a != KEY else "***" for a in cmd))
    with open(log, "wb") as out:
        proc = subprocess.Popen(cmd, stdout=out, stderr=subprocess.STDOUT, start_new_session=True)
    deadline = time.monotonic() + minutes * 60
    mark = time.monotonic() + 60
    n = 0
    while time.monotonic() < deadline:
        if served(port):
            print(label, "준비 완료")
            kv = re.findall(r"(GPU KV cache size: [^\n]+|Maximum concurrency for [^\n]+)", tail(log, 200000))
            for line in kv[-2:]:
                print("  ", line.strip())
            return proc
        if proc.poll() is not None:
            print(tail(log))
            raise RuntimeError(f"{label} vLLM이 끝났습니다(코드 {proc.returncode}).")
        if time.monotonic() >= mark:
            n += 1
            print(f"  {label}: {n}분 경과")
            mark += 60
        time.sleep(5)
    stop(proc)
    print(tail(log))
    raise RuntimeError(f"{label}: {minutes}분 안에 준비되지 않았습니다.")


def cloudflared():
    if CLOUDFLARED.is_file():
        return str(CLOUDFLARED)
    found = shutil.which("cloudflared")
    if found:
        return found
    BIN.mkdir(parents=True, exist_ok=True)
    with urlopen(CF_URL, timeout=120) as r, CLOUDFLARED.open("wb") as out:
        shutil.copyfileobj(r, out)
    CLOUDFLARED.chmod(0o755)
    return str(CLOUDFLARED)


def tunnel(cf, port, label):
    subprocess.run(["pkill", "-f", f"cloudflared.*127.0.0.1:{port}"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    log = WORK / f"cloudflared_{label}.log"
    log.write_bytes(b"")
    proc = subprocess.Popen([cf, "tunnel", "--no-autoupdate", "--url", f"http://127.0.0.1:{port}"],
                            stdout=open(log, "ab"), stderr=subprocess.STDOUT, start_new_session=True)
    pat = re.compile(r"https://[a-z0-9-]+\.trycloudflare\.com")
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        m = pat.search(tail(log, 20000))
        if m:
            return proc, m.group(0)
        if proc.poll() is not None:
            print(tail(log))
            raise RuntimeError(f"{label} 터널이 끝났습니다.")
        time.sleep(1)
    stop(proc)
    raise RuntimeError(f"{label}: 60초 안에 터널 주소를 받지 못했습니다.")


def retry(fn, tries=30):
    last = None
    for _ in range(tries):
        try:
            return fn()
        except urllib.error.HTTPError as e:
            last = f"HTTP {e.code}"
        except Exception as e:
            last = repr(e)
        time.sleep(3)
    raise RuntimeError(f"터널을 거친 요청 실패: {last}")


def check_embed(base):
    r = retry(lambda: http_json(base + "/v1/embeddings", {"model": BGE_NAME, "input": ["계약의 해지", "손해배상의 범위"]}, timeout=30))
    return len(r["data"]), len(r["data"][0]["embedding"])


def check_chat(base):
    t = time.monotonic()
    r = retry(lambda: http_json(base + "/v1/chat/completions", {
        "model": GEMMA_NAME, "max_tokens": 128, "temperature": 0,
        "messages": [{"role": "user", "content": "국가배상법 제2조가 무엇을 정하는지 한 문장으로 답하라."}],
    }, timeout=120))
    text = r["choices"][0]["message"].get("content") or ""
    leak = any(mark in text for mark in ("<think>", "</think>", "<thought", "thought\n"))
    return round(time.monotonic() - t, 1), text[:120], leak


def check_tool(base):
    tool = {"type": "function", "function": {
        "name": "search_laws", "description": "법령 조문을 검색한다.",
        "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}}
    r = retry(lambda: http_json(base + "/v1/chat/completions", {
        "model": GEMMA_NAME, "max_tokens": 256, "temperature": 0, "tools": [tool], "tool_choice": "auto",
        "messages": [{"role": "user", "content": "국가배상법 제2조를 search_laws 도구로 찾아라."}],
    }, timeout=120))
    calls = r["choices"][0]["message"].get("tool_calls") or []
    return [(c["function"]["name"], c["function"]["arguments"]) for c in calls]


gpu()
VLLM = ensure_vllm()
check_repo()
tools = parser_names("tool")
reasons = parser_names("reasoning")
TOOL_PARSER = pick(tools, ["gemma4", "gemma_4", "gemma"])
REASON_PARSER = pick(reasons, ["gemma4", "gemma_4", "gemma"])
print("도구 호출 파서 후보:", tools)
print("생각 파서 후보:", reasons)
if not TOOL_PARSER:
    raise RuntimeError("이 vLLM 판에 Gemma용 도구 호출 파서가 없습니다. 위 후보 목록을 알려 주십시오.")
print("쓸 파서: 도구", TOOL_PARSER, "/ 생각", REASON_PARSER or "(없음 — 창구가 생각 구간을 거름)")

bge_proc = start(VLLM, [BGE_REPO, "--served-model-name", BGE_NAME, "--runner", "pooling",
                        "--max-model-len", "8192", "--gpu-memory-utilization", str(BGE_MEM)], BGE_PORT, "bge")

gemma_args = [GEMMA_REPO, "--served-model-name", GEMMA_NAME,
              "--gpu-memory-utilization", str(GEMMA_MEM),
              "--max-model-len", str(GEMMA_MAX_LEN),
              "--max-num-seqs", str(GEMMA_MAX_SEQS),
              "--max-num-batched-tokens", str(GEMMA_BATCHED_TOKENS),
              "--enable-chunked-prefill", "--enable-prefix-caching",
              "--scheduling-policy", "priority",
              "--enable-auto-tool-choice", "--tool-call-parser", TOOL_PARSER]
if REASON_PARSER:
    gemma_args += ["--reasoning-parser", REASON_PARSER]
gemma_proc = start(VLLM, gemma_args, GEMMA_PORT, "gemma")

CF = cloudflared()
_, EMBED_BASE = tunnel(CF, BGE_PORT, "bge")
_, LLM_BASE = tunnel(CF, GEMMA_PORT, "gemma")

n, dim = check_embed(EMBED_BASE)
secs, sample, leak = check_chat(LLM_BASE)
calls = check_tool(LLM_BASE)

print()
print("확인(터널 주소로):")
print(f"  임베딩: 벡터 {n}개, 차원 {dim}")
print(f"  생성: {secs}초, 답 앞부분: {sample!r}")
print("  생각 글이 답에 섞임:", "예" if leak else "아니오")
print("  도구 호출:", calls if calls else "없음(도구 호출이 나오지 않음 — 파서·모델 확인 필요)")
print()
print("아래를 비공개 .env에 넣으십시오:")
print("LLM_BASE_URL =", LLM_BASE + "/v1")
print("LLM_MODEL    =", GEMMA_NAME)
print("EMBED_BASE_URL =", EMBED_BASE + "/v1")
print("EMBED_MODEL    =", BGE_NAME)
if KEY_NEW:
    print("LLM_API_KEY = EMBED_API_KEY =", KEY)
    print("(이번 실행에서 새로 만든 키입니다. 노트 출력을 공유하지 마십시오. 고정하려면 MODEL_API_KEY 환경변수를 정하십시오.)")
else:
    print("LLM_API_KEY = EMBED_API_KEY = (MODEL_API_KEY 환경변수 값)")
