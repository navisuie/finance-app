import { Card } from "../components/ui";

export function Placeholder({ title, phase, detail }: { title: string; phase: number; detail: string }) {
  return (
    <div className="max-w-3xl space-y-4">
      <h1 className="text-2xl font-semibold">{title}</h1>
      <Card>
        <p className="text-sm font-medium">Arrives in phase {phase}</p>
        <p className="mt-1 text-sm text-zinc-500">{detail}</p>
      </Card>
    </div>
  );
}
