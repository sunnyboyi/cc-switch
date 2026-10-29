import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "../../src/index.css";
import "./web.css";
import "../../src/i18n";
import { initializeWindowActivity } from "@/lib/windowActivity";
import { initializeInputModality } from "@/lib/inputModality";

initializeWindowActivity();
initializeInputModality();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
