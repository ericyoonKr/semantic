import { createContext, useContext } from "react";
import type { Source } from "./events";

export type DocRef = { filename: string; page: number | null };

export type SourceView = {
  shown: Source | null;
  open: (source: Source) => void;
  openDoc: (ref: DocRef) => void;
};

export const SourceViewContext = createContext<SourceView | null>(null);

export function useSourceView(): SourceView | null {
  return useContext(SourceViewContext);
}
