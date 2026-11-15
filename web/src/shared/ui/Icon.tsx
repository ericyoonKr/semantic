const PATHS = {
  plus: ["M12 5v14", "M5 12h14"],
  attach: ["M8 12.5l5.6-5.6a3 3 0 0 1 4.2 4.2l-7.1 7.1a5 5 0 0 1-7.1-7.1L11 3.7"],
  send: ["M12 19V5", "M6 11l6-6 6 6"],
  doc: ["M7 3h7l5 5v13H7z", "M14 3v5h5", "M10 13h6", "M10 17h6"],
  menu: ["M4 7h16", "M4 12h16", "M4 17h16"],
  close: ["M6 6l12 12", "M18 6L6 18"],
  tray: ["M12 4v11", "M7 10l5 5 5-5", "M4 15v4h16v-4"],
  check: ["M5 12.5l4.5 4.5L19 7.5"],
  alert: ["M12 4l9 16H3z", "M12 10v4", "M12 17.2v.1"],
  scale: ["M12 4v16", "M7 20h10", "M5 8h14", "M5 8l-3 6a3 3 0 0 0 6 0z", "M19 8l-3 6a3 3 0 0 0 6 0z"],
  panel: ["M4 5h16v14H4z", "M15 5v14"],
  pen: ["M4 20h4L19 9l-4-4L4 16z", "M13.5 6.5l4 4"],
  trash: ["M5 7h14", "M9 7V5h6v2", "M7 7l1 12h8l1-12", "M10.5 11v5", "M13.5 11v5"],
  redo: ["M20 12a8 8 0 1 1-2.3-5.7", "M20 4v6h-6"],
  up: ["M6 15l6-6 6 6"],
  down: ["M6 9l6 6 6-6"],
} as const;

export type IconName = keyof typeof PATHS;

type Props = {
  name: IconName;
};

export function Icon({ name }: Props) {
  return (
    <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
