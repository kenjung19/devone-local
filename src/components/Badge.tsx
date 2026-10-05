export function Badge({ value }: { value: string }) {
  return (
    <span
      className={
        "badge " +
        (value.toLowerCase() === "running" ||
        value === "trusted" ||
        value === "Ready"
          ? "good"
          : [
                "missing",
                "conflict",
                "unhealthy",
                "Error",
                "Missing folder",
                "failed",
              ].includes(value)
            ? "bad"
            : "")
      }
    >
      {value}
    </span>
  );
}
