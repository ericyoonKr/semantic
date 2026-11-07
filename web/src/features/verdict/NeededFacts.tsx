type Props = {
  asks: string[];
};

export function NeededFacts({ asks }: Props) {
  if (asks.length === 0) {
    return null;
  }
  return (
    <section className="needed" aria-label="더 알려 주실 사실">
      <h3>더 알려 주실 사실</h3>
      <ul>
        {asks.map((ask, i) => (
          <li key={i}>{ask}</li>
        ))}
      </ul>
    </section>
  );
}
