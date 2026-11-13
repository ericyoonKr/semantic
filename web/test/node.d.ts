declare module "node:fs" {
  export function readFileSync(path: string): Uint8Array<ArrayBuffer>;
}

declare module "node:url" {
  export function fileURLToPath(url: URL): string;
}
