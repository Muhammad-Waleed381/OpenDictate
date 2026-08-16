import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MainApp, DockApp } from "./App";
import "./index.css";

document.documentElement.classList.add("dark");

async function main() {
  let isDock = false;
  try {
    isDock = getCurrentWindow().label === "dock";
  } catch {
    isDock = new URLSearchParams(window.location.search).get("window") === "dock";
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      {isDock ? <DockApp /> : <MainApp />}
    </React.StrictMode>,
  );
}

main();