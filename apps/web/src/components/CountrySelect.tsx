import { useRatingSystems } from "@/api/hooks";
import { Select } from "@/components/ui";
import { countryChoices } from "@/lib/access";

/** The server's country: one of those whose ratings Pley knows, by name in the language shown. */
export function CountrySelect({ id, value, onChange }: { id: string; value: string; onChange: (country: string) => void }) {
  const systems = useRatingSystems();
  return (
    <Select id={id} value={value} onChange={(e) => onChange(e.target.value)}>
      {countryChoices(systems.data, value).map((c) => (
        <option key={c.value} value={c.value}>
          {c.label}
        </option>
      ))}
    </Select>
  );
}
