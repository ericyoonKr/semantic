import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { loadWebtext } from "../shared/webtext";
import { App } from "./App";
import "./styles.css";

const root = document.getElementById("root");
if (root) {
  void loadWebtext().then(() =>
    createRoot(root).render(
      <StrictMode>
        <App />
      </StrictMode>,
    ),
  );
}
