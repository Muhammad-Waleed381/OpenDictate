import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MainApp, OverlayApp } from "./App";
import "./index.css";

document.documentElement.classList.add("dark");

async function main() {
  let isOverlay = false;
  try {
    isOverlay = getCurrentWindow().label === "overlay";
  } catch {
    isOverlay =
      new URLSearchParams(window.location.search).get("window") === "overlay";
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {isOverlay ? <OverlayApp /> : <MainApp />}
    </React.StrictMode>,
  );
}

main();