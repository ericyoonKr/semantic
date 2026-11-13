import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { loadWebtextFrom } from "../src/shared/webtext";

loadWebtextFrom(readFileSync(fileURLToPath(new URL("../src/wasm/webtext_bg.wasm", import.meta.url))));
