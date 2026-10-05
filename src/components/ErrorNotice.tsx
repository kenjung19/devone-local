import { userError } from "../presentation";
export function ErrorNotice({ error }: { error: string }) {
  if (!error) return null;
  const message = userError(error);
  return (
    <div className="alert error" role="alert">
      <strong>{message}</strong>
      <details>
        <summary>Technical details</summary>
        <pre>{error}</pre>
      </details>
    </div>
  );
}
