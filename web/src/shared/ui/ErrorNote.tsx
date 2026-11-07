type Props = {
  message: string | null;
  requestId?: string | undefined;
};

export function ErrorNote({ message, requestId }: Props) {
  if (!message) {
    return null;
  }
  return (
    <p className="error" role="alert">
      {message}
      {requestId ? <span className="request-id"> (요청 번호 {requestId})</span> : null}
    </p>
  );
}
