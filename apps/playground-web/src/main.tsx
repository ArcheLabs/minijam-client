import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app";
import "./styles.css";
import faviconUrl from "./minijam.ico";

const favicon = document.createElement("link");
favicon.rel = "icon";
favicon.type = "image/x-icon";
favicon.href = faviconUrl;
document.head.appendChild(favicon);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
