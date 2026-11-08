import re
import unicodedata

INJECTION_KIND = "injection"

_EARLIER = r"(?:이전|이전의|앞의|앞선|앞에서\s*받은|위의|위|기존|기존의|모든|지금까지의|지금까지\s*받은|시스템|원래의|원래)"
_ORDERS = r"(?:지시|지침|명령|규칙|프롬프트|설정|안내|제약|대화)"
_DROP = r"(?:무시|잊어|잊고|잊으|폐기|취소|따르지\s*(?:마|말))"
_AI = r"(?:AI|인공지능|챗봇|어시스턴트|assistant|언어\s*모델|AI\s*모델|에이전트|agent|LLM|GPT)"
_YOU = r"(?:너는|넌|너의|너에게|당신은|당신의|당신에게|네가|" + _AI + r"(?:는|은|야|에게|한테))"
_SAY = r"(?:답하|답해|대답하|말하|말해|출력하|작성하|써|쓰|적어|밝히|공개하|전하|보여\s*주|알려\s*주)"
_ORDER_END = r"(?:라|세요|십시오|해라|해|줘|야\s*한다|야\s*합니다)"
_COMMAND = r"(?:라|세요|십시오|해라|해\s*줘|해\s*주세요|야\s*한다|야\s*합니다|지\s*마|지\s*말)"
_TOOLS = r"(?:도구|tool|judge|search_laws|search_cases|read_law|read_case|case_citations|kg_find|kg_expand|kg_write_state|read_file|write_file|edit_file|grep|write_todos)"

PATTERNS = (
    re.compile(_EARLIER + r"\s*" + _ORDERS + r"\S{0,4}(?:\s*(?:내용|사항|것))?\s*(?:은|는|을|를|들은|들을)?\s*(?:모두\s*|전부\s*|다\s*)?" + _DROP),
    re.compile(
        _ORDERS
        + r"\S{0,4}\s*(?:은|는|을|를)?\s*(?:모두\s*|전부\s*|다\s*)?"
        + r"(?:무시(?:하라|하세요|하십시오|해라|해도|하고\s*(?:다음|아래|이제|지금|새|이\s*문서|이\s*글))|잊어|잊고|잊으세요)"
    ),
    re.compile(r"(?:지시|명령|프롬프트|규칙)\S{0,3}\s*(?:은|는)\s*(?:효력이\s*없|무효)"),
    re.compile(r"새\s*(?:지시|명령|규칙|프롬프트|지침)(?:는|은)?\s*(?::|다음)"),
    re.compile(r"시스템\s*(?:프롬프트|지시)|system\s*prompt", re.IGNORECASE),
    re.compile(r"(?<![가-힣])(?:너의|당신의|네)\s*(?:새\s*)?(?:규칙|지시|역할|임무)"),
    re.compile(r"(?m)^\s*\[?(?:system|assistant|user|시스템|어시스턴트)\]?\s*:", re.IGNORECASE),
    re.compile(r"(?:지금부터|이제부터|이제)\s*" + _YOU),
    re.compile(_YOU + r"\s*(?:이제|지금부터|앞으로)"),
    re.compile(r"(?:역할|임무|정체)\s*(?:은|는|이)?\s*(?:이제|지금부터|앞으로)"),
    re.compile(_YOU + r"[^.\n]{0,60}?" + _ORDER_END + r"(?![가-힣])"),
    re.compile(r"(?<![가-힣])" + _AI + r"\s*(?:에게|에|한테)\s*(?::|\s)"),
    re.compile(_AI + r"\s*(?:에|에게)\s*대한\s*(?:지시|명령|요청)"),
    re.compile(_AI + r"\s*(?:에게|에|는|은)[^.\n]{0,40}?(?:지시|명령|요청)"),
    re.compile(
        r"(?:답변|대답|응답|출력)(?:에|으로|은|는|을|를|\s*끝에|\s*첫머리에)?[^.\n]{0,40}?(?:이라고|라고)\s*(?:\S+\s*)?"
        + _SAY
        + r"\S{0,3}"
        + _ORDER_END
        + r"(?![가-힣])"
    ),
    re.compile(r"(?<![가-힣])(?:답|대답|응답)(?:하라|하세요|하십시오|해라|해\s*줘|해\s*주세요)"),
    re.compile(r"(?:판정|결론|결과)(?:을|를|은|는)?[^.\n]{0,30}?(?:바꿔|뒤집어|고쳐)\s*(?:라|서|줘|주세요)"),
    re.compile(r"출력(?:하라|해라|해\s*줘|해\s*주세요|하세요|하십시오)"),
    re.compile(_TOOLS + r"\s*(?:을|를|은|는)?[^.\n]{0,20}?(?:불러|부르|호출|사용|실행)(?:하|해)?\s*" + _COMMAND),
    re.compile(_TOOLS + r"[^.\n]{0,20}?(?:불러|호출해|실행해)\s*(?:라|줘|주세요)"),
    re.compile(r"\b(?:ignore|disregard|forget)\b[^.\n]{0,30}\b(?:instructions?|prompts?|rules?|directions?)\b", re.IGNORECASE),
    re.compile(r"\byou\s+(?:are\s+now|must\s+now)\b|\bact\s+as\b|\bnew\s+instructions?\b|\bjailbreak\b", re.IGNORECASE),
)


def instruction_hits(text: str) -> int:
    body = unicodedata.normalize("NFC", text)
    return sum(1 for pattern in PATTERNS if pattern.search(body))


def looks_injected(text: str) -> bool:
    return instruction_hits(text) > 0
