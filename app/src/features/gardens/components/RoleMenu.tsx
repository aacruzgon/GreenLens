// A role choice as a menu on its button (screens 3.9, "Role ▾"; HIG Menus, and HIG Alerts: "Use an action sheet —
// not an alert — to offer choices related to an intentional action"). On iOS it is SwiftUI's Menu, as SoilMenu is:
// it opens from the button, with the member's current role checked (HIG Menus, toggled items). Elsewhere, an alert
// with the roles; under Jest the button opens the roles in place, with test ids, since a SwiftUI menu can't be tapped.
import { Button as MenuItem, HStack, Host, Image as SFSymbol, Menu, Section, Text as SwiftText } from "@expo/ui/swift-ui";
import { accessibilityLabel as a11yLabel, buttonStyle, font, foregroundStyle, menuIndicator, menuStyle } from "@expo/ui/swift-ui/modifiers";
import { useState } from "react";
import { Alert, Platform, View } from "react-native";

import type { Role } from "@/domain/types";
import { Button, elevation, minTouch, radii, space, useTheme } from "@/ui";

import { roleName } from "../model/members";

export interface RoleMenuProps {
  /** The button's word: "Role" or "Invite". */
  label: string;
  /** The menu's header, and the fallback alert's title: "Invite as", "Sam Lee's role". */
  title: string;
  /** The fallback alert's message. */
  message?: string;
  roles: readonly Role[];
  /** Checked in the menu; picking it again does nothing. */
  current?: Role;
  onPick: (role: Role) => void;
  variant?: "secondary" | "plain";
  busy?: boolean;
  accessibilityLabel?: string;
  testID?: string;
}

export function RoleMenu({ label, title, message, roles, current, onPick, variant = "plain", busy = false, accessibilityLabel, testID }: RoleMenuProps) {
  const { colors, dark, scheme } = useTheme();
  const [open, setOpen] = useState(false);
  const small = variant === "plain";
  const pick = (role: Role) => {
    setOpen(false);
    if (role !== current) onPick(role);
  };

  if (process.env.NODE_ENV === "test") {
    return (
      <View style={{ gap: space[1] }}>
        <Button
          label={`${label} ▾`}
          variant={variant}
          size={small ? "small" : "regular"}
          busy={busy}
          accessibilityLabel={accessibilityLabel ?? label}
          onPress={() => setOpen((o) => !o)}
          testID={testID}
        />
        {open
          ? roles.map((role) => (
              <Button
                key={role}
                label={role === current ? `✓ ${roleName(role)}` : roleName(role)}
                variant="plain"
                size="small"
                onPress={() => pick(role)}
                testID={testID ? `${testID}-${role}` : undefined}
              />
            ))
          : null}
      </View>
    );
  }

  if (Platform.OS === "ios" && !busy) {
    const size = small ? 15 : 17;
    return (
      <View
        testID={testID}
        style={{
          minHeight: minTouch,
          minWidth: minTouch,
          justifyContent: "center",
          alignItems: "center",
          alignSelf: "flex-start",
          paddingHorizontal: small ? 10 : 18,
          borderRadius: radii.pill,
          backgroundColor: variant === "secondary" ? colors.surface : "transparent",
          boxShadow: variant === "secondary" && !dark ? elevation.pillSecondary : undefined,
        }}
      >
        <Host matchContents colorScheme={scheme}>
          <Menu
            label={
              <HStack spacing={4}>
                <SwiftText modifiers={[font({ size, weight: "semibold" }), foregroundStyle(colors.text)]}>{label}</SwiftText>
                <SFSymbol systemName="chevron.down" size={size - 4} color={colors.text} />
              </HStack>
            }
            modifiers={[menuStyle("button"), buttonStyle("plain"), menuIndicator("hidden"), a11yLabel(accessibilityLabel ?? label)]}
          >
            <Section title={title}>
              {roles.map((role) => (
                <MenuItem
                  key={role}
                  label={roleName(role)}
                  systemImage={role === current ? "checkmark" : undefined}
                  onPress={() => pick(role)}
                />
              ))}
            </Section>
          </Menu>
        </Host>
      </View>
    );
  }

  const ask = () =>
    Alert.alert(title, message, [
      ...roles.filter((r) => r !== current).map((role) => ({ text: roleName(role), onPress: () => pick(role) })),
      { text: "Cancel", style: "cancel" as const },
    ]);
  return (
    <Button
      label={`${label} ▾`}
      variant={variant}
      size={small ? "small" : "regular"}
      busy={busy}
      accessibilityLabel={accessibilityLabel ?? label}
      onPress={ask}
      testID={testID}
    />
  );
}
