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
          <Badge className="bg-[#10B981]/15 text-[#10B981]">
            No telemetry
          </Badge>
        </div>
        <CardDescription>
          Zero outbound calls except model downloads. Audio never leaves this
          machine.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <div className="flex flex-col gap-1.5">
          <span className="font-medium text-[#F8FAFC]">
            Network endpoints
          </span>
          <ul className="flex flex-col gap-1 text-[#64748B]">
            {ENDPOINTS.map((endpoint) => (
              <li key={endpoint} className="flex items-start gap-2">
                <span className="mt-1.5 size-1 shrink-0 rounded-full bg-[#3B82F6]" />
                {endpoint}
              </li>
            ))}
          </ul>
        </div>
        <p className="text-[#64748B]">
          All transcription runs locally via on-device models. Settings,
          history, and your custom dictionary are stored in a single SQLite
          file on your machine. See{" "}
          <span className="cursor-pointer text-[#3B82F6] underline underline-offset-2">
            PRIVACY.md
          </span>{" "}
          for details.
        </p>
      </CardContent>
    </Card>
  );
}