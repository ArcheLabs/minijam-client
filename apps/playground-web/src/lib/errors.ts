import { PlaygroundApiError } from "../api/playground";

export function errorMessage(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (/replayed|already used/i.test(message)) return "This signed action was already used. Prepare and sign a new action.";
  if (/expired/i.test(message)) return "The signed action expired. Please review and sign the operation again.";
  if (/parameters do not match/i.test(message)) return "The submitted parameters do not match the action you signed.";
  if (/signature/i.test(message)) return `Wallet signature error: ${message}`;
  if (error instanceof PlaygroundApiError && error.status === 403) return "The connected account is not the finalized Service Controller.";
  if (error instanceof PlaygroundApiError && error.status === 404) return "The requested Service or Operation was not found.";
  if (/compiler/i.test(message)) return `Compiler error: ${message}`;
  if (/bundle/i.test(message)) return `Bundle error: ${message}`;
  if (/work failed/i.test(message)) return `Work execution failed: ${message}`;
  return message;
}
