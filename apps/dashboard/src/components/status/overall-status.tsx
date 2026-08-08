import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

type Props = {
  title: string;
  subtitle: string;
  variant: "operational" | "degraded" | "outage";
  uptime: number;
  openIncidents: number;
};

function variantClass(variant: Props["variant"]) {
  switch (variant) {
    case "operational":
      return "border-emerald-500/30 bg-emerald-500/10 text-emerald-700";
    case "degraded":
      return "border-amber-500/30 bg-amber-500/10 text-amber-700";
    case "outage":
      return "border-rose-500/30 bg-rose-500/10 text-rose-700";
  }
}

function variantLabel(variant: Props["variant"]) {
  switch (variant) {
    case "operational":
      return "Operational";
    case "degraded":
      return "Degraded";
    case "outage":
      return "Outage";
  }
}

export function OverallStatus({
  title,
  subtitle,
  variant,
  uptime,
  openIncidents,
}: Props) {
  return (
    <Card className={cn("overflow-hidden", variantClass(variant))}>
      <CardHeader className="space-y-2">
        <div className="flex items-center justify-between gap-4">
          <CardTitle className="text-2xl font-bold">{title}</CardTitle>
          <Badge className="rounded-full px-3 py-1">{variantLabel(variant)}</Badge>
        </div>
        <p className="text-sm text-muted-foreground">{subtitle}</p>
      </CardHeader>

      <CardContent className="grid gap-4 md:grid-cols-3">
        <div>
          <p className="text-sm text-muted-foreground">Overall uptime</p>
          <p className="mt-2 text-3xl font-bold">{uptime.toFixed(2)}%</p>
        </div>
        <div>
          <p className="text-sm text-muted-foreground">Open incidents</p>
          <p className="mt-2 text-3xl font-bold">{openIncidents}</p>
        </div>
        <div>
          <p className="text-sm text-muted-foreground">System state</p>
          <p className="mt-2 text-3xl font-bold">{variantLabel(variant)}</p>
        </div>
      </CardContent>
    </Card>
  );
}
