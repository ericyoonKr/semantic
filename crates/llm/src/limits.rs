// 동시성 제한 (PROJECT.md §10)
//
// 담을 것:
// - struct LlmLimiter(Arc<tokio::sync::Semaphore>)  // 기본 8~16, 설정으로
// - impl: async fn acquire(&self) -> OwnedSemaphorePermit
//
// 쓰임: engine::leaf의 fan-out이 리프마다 permit을 잡고 호출한다.
//       API 한도(분당 요청·토큰)에 맞춰 값을 정하고, 왜 그 값인지 주석으로 남긴다.
//
// 취소: CancellationToken은 여기가 아니라 호출자(leaf.rs)가 select!로 건다 —
//       permit 대기 중에도 취소가 먹어야 하므로.
