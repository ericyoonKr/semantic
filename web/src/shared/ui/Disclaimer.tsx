export const DISCLAIMER_TEXT = "법률 자문이 아님·변호사 검토 아님·평가용";

export function Disclaimer() {
  return (
    <div className="disclaimer" role="note">
      {DISCLAIMER_TEXT}
    </div>
  );
}
