import { View } from "react-native";

import { Button, Card, ErrorState, ListItem, Screen, Section, Skeleton, Text, space, useTheme } from "@/ui";

import { BackHeader } from "../components/BackHeader";
import { useMembersScreen } from "../hooks/useMembersScreen";

// Members and invites (screens 3.9; flows 3). Owners change roles, remove members and manage invites; any member
// who isn't the last owner can leave. Actions that would fail are hidden, not disabled (README 9, roles in the UI).
export function MembersScreen({ gardenId }: { gardenId: string }) {
  const { colors } = useTheme();
  const vm = useMembersScreen(gardenId);
  const { actions } = vm;

  let body;
  if (vm.loading) {
    body = (
      <View style={{ gap: space[3] }}>
        <Skeleton height={52} />
        <Skeleton height={52} />
      </View>
    );
  } else if (vm.error) {
    body = <ErrorState onRetry={vm.retry} />;
  } else {
    body = (
      <>
        <Section title="Members">
          <Card padded={false}>
            {vm.rows.map((row, i) => (
              <ListItem
                key={row.member.user_id}
                title={row.isMe ? `${row.member.display_name} (you)` : row.member.display_name}
                subtitle={row.subtitle}
                style={i > 0 ? { borderTopWidth: 1, borderTopColor: colors.border } : undefined}
                trailing={
                  row.canManage ? (
                    <View style={{ flexDirection: "row", gap: space[1] }}>
                      <Button
                        label="Role"
                        variant="plain"
                        size="small"
                        accessibilityLabel={`Change ${row.member.display_name}'s role`}
                        onPress={() => actions.changeRole(row.member)}
                        testID={`role-${row.member.user_id}`}
                      />
                      <Button
                        label="Remove"
                        variant="plain"
                        size="small"
                        accessibilityLabel={`Remove ${row.member.display_name}`}
                        onPress={() => actions.removeMember(row.member)}
                        testID={`remove-${row.member.user_id}`}
                      />
                    </View>
                  ) : undefined
                }
              />
            ))}
          </Card>
        </Section>

        {vm.isOwner ? (
          <Section title="Invites">
            {vm.inviteRows.length > 0 ? (
              <Card padded={false}>
                {vm.inviteRows.map((row, i) => (
                  <ListItem
                    key={row.invite.id}
                    title={row.subtitle}
                    style={i > 0 ? { borderTopWidth: 1, borderTopColor: colors.border } : undefined}
                    trailing={
                      <View style={{ flexDirection: "row", gap: space[1] }}>
                        <Button label="Share" variant="plain" size="small" onPress={() => void actions.share(row.invite)} testID={`share-${row.invite.id}`} />
                        <Button
                          label="Revoke"
                          variant="plain"
                          size="small"
                          accessibilityLabel={`Revoke the ${row.subtitle} invite`}
                          onPress={() => actions.revokeInvite(row.invite)}
                          testID={`revoke-${row.invite.id}`}
                        />
                      </View>
                    }
                  />
                ))}
              </Card>
            ) : vm.invitesLoading ? (
              <Skeleton height={52} />
            ) : (
              <Text variant="footnote" color="textMuted">
                No open invites. An invite is a link that works once, for 7 days.
              </Text>
            )}
            <Button label="Invite" variant="secondary" onPress={actions.chooseInviteRole} busy={vm.inviting} testID="invite-add" />
          </Section>
        ) : null}

        {vm.canLeave ? <Button label="Leave garden" variant="secondary" onPress={actions.leave} testID="leave-garden" /> : null}
        {!vm.canLeave && vm.isOwner ? (
          <Text variant="footnote" color="textMuted">
            {"You're the only owner, so you can't leave. Make someone else an owner first."}
          </Text>
        ) : null}
      </>
    );
  }

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="members-screen">
      <BackHeader title={vm.garden ? `Members · ${vm.garden.name}` : "Members"} onBack={actions.back} />
      <Screen underHeader>
        <View style={{ gap: space[6], paddingTop: space[4] }}>{body}</View>
      </Screen>
    </View>
  );
}
