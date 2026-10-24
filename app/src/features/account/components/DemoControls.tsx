import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { View } from "react-native";

import { chooseAccount, guestInviteCode, mockStore, useScenario, type ChatFault, type DemoAccount, type NextScan, type UploadFault } from "@/api/mock";
import type { Role } from "@/domain/types";
import { Chip, Section, Text, space } from "@/ui";

// Mock mode's demo controls (README §15, fault injection): what the next scan does, a failed upload, a dropped stream
// or a used-up budget (chat's Resend and its budget message, F6), the account and the role. The account and the role change what the server answers, so both refetch every query; the next
// scan and the next upload are read only when they happen.
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

/** The next stream (a chat answer or a scan's events) drops after a few events; or the next chat message is over budget. */
type NextFault = "none" | "drop" | ChatFault;
const NEXT_FAULTS: { value: NextFault; label: string }[] = [
  { value: "none", label: "None" },
  { value: "drop", label: "Stream drops" },
  { value: "budget_exceeded", label: "Chat budget used up" },
  { value: "error_event", label: "Chat answer errors" },
  { value: "stall", label: "Chat goes quiet" },
  { value: "in_progress", label: "Chat reply lost" },
];

const ACCOUNTS: { value: DemoAccount; label: string }[] = [
  { value: "demo", label: "The demo garden" },
  // No name and no garden, so the session gate opens onboarding.
  { value: "new", label: "A new account" },
  // Someone else with no garden, who opens the owner's newest invite to the demo garden (roadmap F7 exit checks).
  { value: "guest", label: "A second account, invited" },
];

const ROLES: Role[] = ["owner", "editor", "reviewer", "viewer"];

export function DemoControls() {
  const s = useScenario();
  const queryClient = useQueryClient();
  const router = useRouter();
  const refetchAll = () => void queryClient.invalidateQueries();
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
        <ChipRow label="Next stream or call">
          {NEXT_FAULTS.map((o) => {
            const current: NextFault = s.dropNextStream ? "drop" : (s.chatFault ?? "none");
            return (
              <Chip
                key={o.value}
                label={o.label}
                selected={current === o.value}
                onPress={() => {
                  s.setDropNextStream(o.value === "drop");
                  s.setChatFault(o.value === "none" || o.value === "drop" ? null : o.value);
                }}
              />
            );
          })}
        </ChipRow>
        <ChipRow label="Account">
          {ACCOUNTS.map((o) => (
            <Chip
              key={o.value}
              label={o.label}
              selected={s.account === o.value}
              onPress={() => {
                const was = s.account;
                chooseAccount(o.value);
                // A different person on the phone: nothing of the last one's cache stays.
                if (o.value === "guest" || was === "guest") queryClient.clear();
                refetchAll();
                if (o.value === "guest" && was !== "guest") {
                  router.push({ pathname: "/invite/[code]", params: { code: guestInviteCode(mockStore) } });
                }
              }}
            />
          ))}
        </ChipRow>
        <ChipRow label="Your role">
          {ROLES.map((r) => (
            <Chip
              key={r}
              label={r[0].toUpperCase() + r.slice(1)}
              selected={s.role === r}
              onPress={() => {
                s.setRole(r);
                refetchAll();
              }}
            />
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
