import { useLocalSearchParams } from "expo-router";

import { ScanScreen } from "@/features/scans";

/** Keyed by the scan, so a screen whose params change to another scan starts that scan's stream afresh. */
export default function ScanRoute() {
  const { scanId } = useLocalSearchParams<{ scanId: string }>();
  return <ScanScreen key={scanId} />;
}
