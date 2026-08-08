import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

type Props = {
  status: "Up" | "Down" | "Idle";
};

export function UptimeBadge({ status }: Props) {
  const className =
    status === "Up"
      ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-700"
      : status === "Down"
        ? "border-rose-500/30 bg-rose-500/15 text-rose-700"
        : "border-border bg-muted text-muted-foreground";

  return (
    <Badge className={cn("rounded-full px-3 py-1 font-medium", className)}>
      {status}
    </Badge>
  );
}
