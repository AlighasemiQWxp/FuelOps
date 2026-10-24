import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createBrowserRouter, RouterProvider } from "react-router";
import { HandleProvider } from "../../HandleSystem/react";
import { AuthProvider } from "../../AuthModule/frontend";
import App from "./App";
import "@fontsource/vazirmatn/400.css";
import "@fontsource/vazirmatn/500.css";
import "@fontsource/vazirmatn/600.css";
import "@fontsource/vazirmatn/700.css";
import "./styles.css";
const client = new QueryClient({
  defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: true } },
});
const router = createBrowserRouter([{ path: "*", element: <App /> }]);
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={client}>
      <HandleProvider>
        <AuthProvider>
          <RouterProvider router={router} />
        </AuthProvider>
      </HandleProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
