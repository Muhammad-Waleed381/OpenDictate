import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";

const ENDPOINTS = [
  "Hugging Face — STT + VAD model files (first run only)",
  "GitHub releases — model bundle updates (first run only)",
];

export function PrivacyTab() {
  return (
    <Card className="max-w-xl">
      <CardHeader>
        <div className="flex items-center gap-2">
          <CardTitle>Privacy</CardTitle>
          <Badge>No telemetry</Badge>
        </div>
        <CardDescription>
          Zero outbound calls except model downloads. Audio never leaves this
          machine.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <div className="flex flex-col gap-1.5">
          <span className="font-bold uppercase tracking-wider">
            Network endpoints
          </span>
          <ul className="flex flex-col gap-1 text-muted-foreground">
            {ENDPOINTS.map((endpoint) => (
              <li key={endpoint} className="flex items-start gap-2">
                <span className="mt-1 size-2 shrink-0 border border-black bg-black" />
                {endpoint}
              </li>
            ))}
          </ul>
        </div>
        <p className="text-muted-foreground">
          All transcription runs locally via on-device models. Settings,
          history, and your custom dictionary are stored in a single SQLite
          file on your machine. See{" "}
          <span className="cursor-pointer font-bold text-foreground underline underline-offset-2">
            PRIVACY.md
          </span>{" "}
          for details.
        </p>
      </CardContent>
    </Card>
  );
}