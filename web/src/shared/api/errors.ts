export const FALLBACK_TEXT = "요청을 처리하지 못했습니다";
export const OFFLINE_TEXT = "서버에 연결하지 못했습니다. 잠시 뒤 다시 시도해 주세요";
export const SIGN_IN_TEXT = "로그인이 필요합니다";

export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export async function errorFrom(res: Response): Promise<ApiError> {
  let message = FALLBACK_TEXT;
  try {
    const body: unknown = await res.json();
    if (body && typeof body === "object" && "message" in body && typeof body.message === "string" && body.message.trim() !== "") {
      message = body.message;
    }
  } catch {
    message = FALLBACK_TEXT;
  }
  return new ApiError(res.status, message);
}

export function messageOf(error: unknown): string {
  if (error instanceof ApiError) {
    return error.message;
  }
  return FALLBACK_TEXT;
}
