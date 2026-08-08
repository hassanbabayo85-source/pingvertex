import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

export type StatusIncident = {
  id: string;
  serviceName: string;
  started_at: string;
  resolved_at: string | null;
};

type Props = {
  incidents: StatusIncident[];
};

function formatDate(value: string | null) {
  if (!value) return "—";

  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

export function IncidentHistory({ incidents }: Props) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent incidents</CardTitle>
      </CardHeader>

      <CardContent className="space-y-4">
        {incidents.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No incidents recorded.
          </p>
        ) : (
          incidents.map((incident) => (
            <div
              key={incident.id}
              className="rounded-2xl border p-4"
            >
              <div className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
                <div>
                  <div className="font-medium">{incident.serviceName}</div>
                  <div className="text-sm text-muted-foreground">
                    Started: {formatDate(incident.started_at)}
                  </div>
                </div>

                <Badge
                  className={
                    incident.resolved_at
                      ? "rounded-full border-emerald-500/30 bg-emerald-500/15 px-3 py-1 text-emerald-700"
                      : "rounded-full border-rose-500/30 bg-rose-500/15 px-3 py-1 text-rose-700"
                  }
                >
                  {incident.resolved_at ? "Resolved" : "Open"}
                </Badge>
              </div>

              <div className="mt-3 text-sm text-muted-foreground">
                Resolved: {formatDate(incident.resolved_at)}
              </div>
            </div>
          ))
        )}
      </CardContent>
    </Card>
  );
}
