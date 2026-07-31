import { useEffect, useState } from "react";
import { PlaygroundPage } from "./pages/playground";
import { OperationPage } from "./pages/operation";
import { ServicePage } from "./pages/service";
import { ServicesPage } from "./pages/services";

export function navigate(path: string) {
  history.pushState({}, "", path);
  dispatchEvent(new PopStateEvent("popstate"));
}

export function App() {
  const [, setRoute] = useState(window.location.pathname);
  useEffect(() => {
    const update = () => setRoute(window.location.pathname);
    addEventListener("popstate", update);
    return () => removeEventListener("popstate", update);
  }, []);
  const path = window.location.pathname;
  const operation = path.match(/^\/operations\/([^/]+)$/);
  const service = path.match(/^\/services\/(\d+)$/);

  let page = <PlaygroundPage />;
  if (operation) page = <OperationPage operationId={operation[1]} />;
  if (path === "/services") page = <ServicesPage />;
  if (service) page = <ServicePage serviceId={Number(service[1])} />;
  return <WalletProvider>{page}</WalletProvider>;
}
import { WalletProvider } from "./wallet/context";
