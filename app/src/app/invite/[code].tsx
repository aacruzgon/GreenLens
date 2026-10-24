import { useLocalSearchParams } from "expo-router";

import { InviteScreen } from "@/features/gardens";

// Outside the auth guards (screens 1; flows 3): an invite link opens here signed in or out.
export default function Invite() {
  const { code } = useLocalSearchParams<{ code: string }>();
  return <InviteScreen code={code ?? ""} />;
}
