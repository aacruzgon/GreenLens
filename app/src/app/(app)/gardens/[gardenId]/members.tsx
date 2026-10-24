import { useLocalSearchParams } from "expo-router";

import { MembersScreen } from "@/features/gardens";

export default function GardenMembers() {
  const { gardenId } = useLocalSearchParams<{ gardenId: string }>();
  return <MembersScreen gardenId={gardenId} />;
}
