import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, HashRouter } from "react-router-dom";
import { Toaster } from "sonner";
import App from "./App";
import { WebDashboardApp } from "./web-dashboard/WebDashboardApp";
import "./index.css";
// Initialize i18n before rendering the app
import "./i18n";
// Initialize Catppuccin theme before rendering so there's no flash
import { useThemeStore } from "./stores/themeStore";

// Apply theme synchronously before React renders to prevent flash of wrong theme
useThemeStore.getState().init();

const app =
  import.meta.env.MODE === "web" ? (
    // 웹 대시보드는 정적 서빙(파일/단일 포트)에서도 라우팅이 깨지지 않도록 해시 라우터를 쓴다.
    <HashRouter>
      <WebDashboardApp />
    </HashRouter>
  ) : (
    <BrowserRouter>
      <App />
    </BrowserRouter>
  );

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    {app}
    <Toaster position="bottom-right" richColors />
  </React.StrictMode>
);
