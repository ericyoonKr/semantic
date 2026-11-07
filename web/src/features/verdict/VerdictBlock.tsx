type Props = {
  text: string;
};

export function VerdictBlock({ text }: Props) {
  return (
    <section className="verdict" aria-label="판정">
      <h3>판정</h3>
      <p className="verdict-text">{text}</p>
    </section>
  );
}
