# infer — 모델 서버 기동 노트 (제출하지 않음)

모델은 사용자 개인 Colab에서 vLLM으로 띄우고 OpenAI 호환 API로 부른다. 설계는 `docs/설계서.md` §2.2.

- 프로세스 둘: 생성(Gemma 4, `--served-model-name gemma4`)과 임베딩(BGE-M3, pooling 실행, `--served-model-name bge-m3`). GPU 메모리를 나눠 쓴다.
- 생성 서버 인자: `--max-model-len`, `--max-num-seqs`, `--max-num-batched-tokens`, `--enable-chunked-prefill`, `--enable-prefix-caching`,
  `--enable-auto-tool-choice --tool-call-parser <Gemma 4용>`, `--scheduling-policy priority`, `--api-key`(환경변수). 값은 0단계 실측 뒤 여기에 적는다.
- 로컬에서는 터널로 부른다. 주소와 키는 이 파일에 적지 않고 환경변수(`LLM_BASE_URL`, `EMBED_BASE_URL`, `LLM_API_KEY`)로만 둔다.
- 모델 가중치는 Colab 쪽 영구 저장소에 캐시해 세션이 끊겨도 다시 받지 않는다.
- 잴 것(0단계): 동시 요청 1·4·8·16의 처리량·지연, 접두 캐시 효과, 임베딩 배치 크기별 처리량, 터널 응답 시간 상한, 같은 문장 재임베딩 cosine.
