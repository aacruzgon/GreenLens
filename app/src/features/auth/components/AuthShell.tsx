import type { ReactNode } from "react";
import { KeyboardAvoidingView, Platform, View } from "react-native";
import { Ionicons } from "@expo/vector-icons";

import { Screen, Text, useTheme } from "@/ui";

/** The frame shared by the sign-in screens: the mark, a title and a short line. */
export function AuthShell({ title, subtitle, children }: { title: string; subtitle?: string; children: ReactNode }) {
  const t = useTheme();
  return (
    <KeyboardAvoidingView style={{ flex: 1 }} behavior={Platform.OS === "ios" ? "padding" : undefined}>
      <Screen padTop contentContainerStyle={{ maxWidth: 440, paddingTop: t.space.xxxl, gap: t.space.xl }}>
        <View style={{ gap: t.space.md }}>
          <View
            style={{ width: 56, height: 56, borderRadius: 28, backgroundColor: t.colors.accent, alignItems: "center", justifyContent: "center" }}
            accessibilityElementsHidden
          >
            <Ionicons name="leaf" size={30} color={t.colors.accentText} />
          </View>
          <Text variant="display" accessibilityRole="header">
            {title}
          </Text>
          {subtitle ? <Text color="textMuted">{subtitle}</Text> : null}
        </View>
        {children}
      </Screen>
    </KeyboardAvoidingView>
  );
}
