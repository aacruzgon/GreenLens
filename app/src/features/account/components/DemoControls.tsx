import { useQueryClient } from "@tanstack/react-query";
import { View } from "react-native";

import { useScenario, type NextScan, type UploadFault } from "@/api/mock";
import type { Role } from "@/domain/types";
import { Chip, Section, Text, space } from "@/ui";

// Mock mode's demo controls (README §15, fault injection): what the next scan does, a failed upload, the role.
const NEXT_SCANS: { value: NextScan; label: string }[] = [
  { value: "auto", label: "The demo story" },
  { value: "early_blight", label: "Early blight" },
  { value: "healthy", label: "Healthy" },
  { value: "low_evidence", label: "Can't tell" },
  { value: "no_plant_found", label: "No plant" },
];

const UPLOAD_FAULTS: { value: UploadFault | null; label: string }[] = [
  { value: null, label: "None" },
  { value: "image_quality", label: "Blurry photo" },
  { value: "image_too_dark", label: "Too dark" },
];

const ROLES: Role[] = ["owner", "editor", "reviewer", "viewer"];

export function DemoControls() {
  const s = useScenario();
  const queryClient = useQueryClient();
  return (
    <Section title="Demo controls">
      <View style={{ gap: space[4] }}>
        <ChipRow label="Next scan">
          {NEXT_SCANS.map((o) => (
            <Chip key={o.value} label={o.label} selected={s.nextScan === o.value} onPress={() => s.setNextScan(o.value)} />
          ))}
        </ChipRow>
        <ChipRow label="Next upload">
          {UPLOAD_FAULTS.map((o) => (
            <Chip key={o.label} label={o.label} selected={s.uploadFault === o.value} onPress={() => s.setUploadFault(o.value)} />
          ))}
        </ChipRow>
        <ChipRow label="Account">
          <Chip label="The demo garden" selected={!s.emptyAccount} onPress={() => s.setEmptyAccount(false)} />
          <Chip
            label="A new account"
            selected={s.emptyAccount}
            onPress={() => {
              // No name and no garden, so the session gate opens onboarding.
              s.setEmptyAccount(true);
              void queryClient.invalidateQueries();
            }}
          />
        </ChipRow>
        <ChipRow label="Your role">
          {ROLES.map((r) => (
            <Chip key={r} label={r[0].toUpperCase() + r.slice(1)} selected={s.role === r} onPress={() => s.setRole(r)} />
          ))}
        </ChipRow>
      </View>
    </Section>
  );
}

function ChipRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <View style={{ gap: space[2] }}>
      <Text variant="subheadline" color="textMuted">
        {label}
      </Text>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>{children}</View>
    </View>
  );
}
