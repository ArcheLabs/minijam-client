import { PlaygroundPage } from "./pages/playground";
import { OperationPage } from "./pages/operation";
import { ServicePage } from "./pages/service";

export function navigate(path: string) {
  history.pushState({}, "", path);
  dispatchEvent(new PopStateEvent("popstate"));
}

export function App() {
  const path = window.location.pathname;
  const operation = path.match(/^\/operations\/([^/]+)$/);
  const service = path.match(/^\/services\/(\d+)$/);

  if (operation) return <OperationPage operationId={operation[1]} />;
  if (service) return <ServicePage serviceId={Number(service[1])} />;
  return <PlaygroundPage />;
}
