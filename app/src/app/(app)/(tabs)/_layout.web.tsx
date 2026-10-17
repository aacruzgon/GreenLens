import { Tabs } from "expo-router";
import { Ionicons } from "@expo/vector-icons";

import { useTheme } from "@/ui";

// The web build: a bottom tab bar in the page's flow, so it never covers content. Native tabs on the
// web float over the top of every tab (PR #42 review). iOS and Android keep the system tab bar.
export default function TabsLayout() {
  const t = useTheme();
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: t.colors.accent,
        tabBarInactiveTintColor: t.colors.textMuted,
        tabBarStyle: { backgroundColor: t.colors.surface, borderTopColor: t.colors.border },
        sceneStyle: { backgroundColor: t.colors.background },
      }}
    >
      <Tabs.Screen
        name="index"
        options={{ title: "Garden", tabBarIcon: ({ color, size, focused }) => <Ionicons name={focused ? "leaf" : "leaf-outline"} color={color} size={size} /> }}
      />
      <Tabs.Screen
        name="settings"
        options={{ title: "Settings", tabBarIcon: ({ color, size, focused }) => <Ionicons name={focused ? "settings" : "settings-outline"} color={color} size={size} /> }}
      />
    </Tabs>
  );
}
