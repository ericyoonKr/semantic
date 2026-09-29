#!/usr/bin/env bash
# ② 추론 서버 기동 — llama.cpp 임베딩 서버 (PROJECT.md §5 ②)
#
# ★★ 착수 전 실측 1 (§12): 코퍼스 임베딩(조문 30만·판례 412만 행)을 만든 모델과
#    동일한 모델을 띄워야 한다. 확인 전에는 이 스크립트를 확정하지 않는다.
#     embed_set 메타데이터로 실측 확인할 것)
#
# 예시 (모델 확정 후 채움):
#   llama-server \
#     --model models/<확정된-임베딩-모델>.gguf \
#     --embedding \
#     --port 8090 \
#     --threads <코어 수에 맞춰> \
#     --batch-size <문서 청크 배치에 맞춰>
#
# 대안: onnxruntime 서빙 (같은 HTTP 계약으로 — retrieval::embed_client가 base_url만 봄)
# 리랭커는 후순위 (§5 ②) — 필요해지면 별도 포트로 추가.
