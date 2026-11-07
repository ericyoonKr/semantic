import { uploadLabel } from "./api";

type Props = {
  lines: string[];
};

export function UploadList({ lines }: Props) {
  return (
    <section className="upload-list" aria-label="올린 파일">
      <h3>올린 파일</h3>
      {lines.length === 0 ? (
        <p className="muted">아직 올린 파일이 없습니다.</p>
      ) : (
        <ul>
          {lines.map((line, i) => (
            <li key={i}>{uploadLabel(line)}</li>
          ))}
        </ul>
      )}
    </section>
  );
}
