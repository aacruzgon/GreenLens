// The development build's tools on a LiDAR iPhone (roadmap F5): the LiDAR day-1 check with the evaluation captures,
// the garden mapping session, and the 3D garden spike. Only in a development or evaluation build
// (EXPO_PUBLIC_DEV_TOOLS=1), and hidden where the LiDAR module is absent (Expo Go, the web).
import { useRouter } from "expo-router";

import { devTools } from "@/config/env";
import { useCapabilities } from "@/platform";
import { ListItem, Section } from "@/ui";

export function DeviceTools() {
  const router = useRouter();
  const { depth, mapping } = useCapabilities();
  if (!devTools || (!depth && !mapping)) return null;
  return (
    <Section title="LiDAR">
      {depth ? (
        <ListItem title="Depth check" subtitle="Measures the depth camera and saves evaluation photos to Files" onPress={() => router.push("/dev/depth-check")} />
      ) : null}
      {mapping ? <ListItem title="Map the garden" subtitle="Beds and plant pins, with LiDAR" onPress={() => router.push("/garden/map-session")} /> : null}
      <ListItem title="3D garden" subtitle="The garden view's spike" onPress={() => router.push("/dev/garden-3d")} />
    </Section>
  );
}
