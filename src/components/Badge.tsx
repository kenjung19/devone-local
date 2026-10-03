export function Badge({ value }: { value: string }) {
  return (
    <span
      className={
        "badge " +
        (value === "running" || value === "trusted"
          ? "good"
          : value === "missing" || value === "conflict" || value === "unhealthy"
            ? "bad"
            : "")
      }
    >
      {value}
    </span>
  );
}
