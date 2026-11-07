const PATHS = {
  plus: ["M12 5v14", "M5 12h14"],
  attach: ["M8 12.5l5.6-5.6a3 3 0 0 1 4.2 4.2l-7.1 7.1a5 5 0 0 1-7.1-7.1L11 3.7"],
  send: ["M12 19V5", "M6 11l6-6 6 6"],
  doc: ["M7 3h7l5 5v13H7z", "M14 3v5h5", "M10 13h6", "M10 17h6"],
  menu: ["M4 7h16", "M4 12h16", "M4 17h16"],
  close: ["M6 6l12 12", "M18 6L6 18"],
  tray: ["M12 4v11", "M7 10l5 5 5-5", "M4 15v4h16v-4"],
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
