import { useEffect, useRef } from "react";

export interface ConfirmDetail {
  label: string;
  value: string;
}

export function ConfirmAction({
  title,
  details,
  busy,
  onConfirm,
  onClose
}: {
  title: string;
  details: ConfirmDetail[];
  busy: boolean;
  onConfirm(): void;
  onClose(): void;
}) {
  const confirm = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    confirm.current?.focus();
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    addEventListener("keydown", close);
    return () => removeEventListener("keydown", close);
  }, [busy, onClose]);
  return (
    <div className="modal-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && !busy) onClose();
    }}>
      <section className="confirm-modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
        <p className="eyebrow">Wallet signature required</p>
        <h2 id="confirm-title">{title}</h2>
        <dl className="confirm-details">
          {details.map((detail) => <div key={detail.label}><dt>{detail.label}</dt><dd className="mono">{detail.value}</dd></div>)}
        </dl>
        <p className="muted">The API remains the authorization source. Your wallet signs only this exact operation.</p>
        <div className="modal-actions">
          <button className="text-button" disabled={busy} onClick={onClose}>Cancel</button>
          <button ref={confirm} className="primary-button" disabled={busy} onClick={onConfirm}>
            {busy ? "Waiting for signature…" : "Confirm & sign"}
          </button>
        </div>
      </section>
    </div>
  );
}
