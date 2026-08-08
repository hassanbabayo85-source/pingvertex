import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export interface Incident {
  id: string;
  started_at: string;
  resolved_at: string | null;
}

interface Props {
  incidents: Incident[];
}

export function IncidentTimeline({ incidents }: Props) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent Incidents</CardTitle>
      </CardHeader>

      <CardContent className="space-y-4">
        {incidents.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No incidents found.
          </p>
        )}

        {incidents.map((incident) => (
          <div
            key={incident.id}
            className="border-l-2 pl-4"
          >
            <div className="font-medium">
              Started
            </div>

            <div className="text-sm text-muted-foreground">
              {new Date(incident.started_at).toLocaleString()}
            </div>

            <div className="mt-1 text-sm">
              {incident.resolved_at
                ? `Resolved: ${new Date(
                    incident.resolved_at
                  ).toLocaleString()}`
                : "Still Active"}
            </div>
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
