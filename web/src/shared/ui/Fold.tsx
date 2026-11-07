import { useId, useState, type ReactNode } from "react";

type Props = {
  long: boolean;
  children: ReactNode;
};

export function Fold({ long, children }: Props) {
  const [open, setOpen] = useState(false);
  const id = useId();
  if (!long) {
    return <>{children}</>;
  }
  return (
    <div className="fold">
      <div id={id} className={open ? "fold-body" : "fold-body fold-closed"}>
        {children}
      </div>
      <button type="button" className="link-button" aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}>
        {open ? "접기" : "펼치기"}
      </button>
    </div>
  );
}
