export function ErrorPanel({ message }: { message?: string | null }) {
  if (!message) return null;
  return (
    <div className="error-panel" role="alert">
      <strong>Action could not be completed</strong>
      <span>{message}</span>
    </div>
  );
}
