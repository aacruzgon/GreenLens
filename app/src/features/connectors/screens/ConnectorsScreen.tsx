// The Farm connectors screen in the garden's settings (EON E10.1 and E10.2; docs/connectors/README.md 2 and 5;
// roadmap F7 item 4): the garden's Slack and Google Drive connectors with their status, account, sources, last sync
// and record count. Owners connect, choose the sources, sync, reconnect and disconnect; every other role reads.
// The demo garden's `era` connectors are labelled as synthetic and offer no actions (EON decisions 10 and 11).
import { useRouter } from "expo-router";
import { useState } from "react";
import { Alert, RefreshControl, View } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";

import { isApiError, useConnectors, useDeleteConnector, useGarden, useSyncConnector } from "@/api";
import { can } from "@/domain/permissions";
import type { Connector, ConnectorKind } from "@/domain/types";
import { Banner, Button, EmptyState, ErrorState, IconButton, LineIcon, Screen, Section, Skeleton, Text, space, useTheme } from "@/ui";

import { ConnectorCard } from "../components/ConnectorCard";
import { SourcePicker } from "../components/SourcePicker";
import { useConnectFlow } from "../hooks/useConnectFlow";
import { connectorView, errorText, VENDOR, type ConnectorAction } from "../model/view";

const KINDS: ConnectorKind[] = ["slack", "google_drive"];

export function ConnectorsScreen({ gardenId }: { gardenId: string }) {
  const router = useRouter();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const garden = useGarden(gardenId);
  const list = useConnectors(gardenId);
  const sync = useSyncConnector();
  const remove = useDeleteConnector();
  const [picker, setPicker] = useState<Connector>();
  const [running, setRunning] = useState<{ id: string; action: ConnectorAction }>();
  const [notice, setNotice] = useState<{ tone: "info" | "danger"; title: string; message?: string }>();
  const flow = useConnectFlow(gardenId, (c) => setPicker(c));

  const role = garden.data?.my_role;
  const owner = can(role, "connectors.manage");
  const timeZone = garden.data?.time_zone ?? "UTC";

  const disconnect = (c: Connector) =>
    Alert.alert(
      `Disconnect ${VENDOR[c.kind]}?`,
      "PDM revokes its access and deletes every message and document it synced for this garden.",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Disconnect",
          style: "destructive",
          onPress: () => {
            setRunning({ id: c.id, action: "disconnect" });
            remove.mutate(c, {
              onError: (e) => setNotice({ tone: "danger", title: "Not disconnected", message: errorText(isApiError(e) ? e : undefined, c.kind) }),
              onSettled: () => setRunning(undefined),
            });
          },
        },
      ],
    );

  const onAction = (action: ConnectorAction, c: Connector) => {
    setNotice(undefined);
    flow.clearError();
    switch (action) {
      case "choose":
        setPicker(c);
        return;
      case "sync":
        setRunning({ id: c.id, action });
        sync
          .mutateAsync(c)
          .then(() => setNotice({ tone: "info", title: "Syncing", message: `PDM is reading what changed in ${VENDOR[c.kind]}.` }))
          .catch((e: unknown) => setNotice({ tone: "danger", title: "Not synced", message: errorText(isApiError(e) ? e : undefined, c.kind) }))
          .finally(() => setRunning(undefined));
        return;
      case "reconnect":
        void flow.start(c.kind, c);
        return;
      case "disconnect":
        disconnect(c);
        return;
      case "cancel":
        setRunning({ id: c.id, action });
        remove.mutate(c, { onSettled: () => setRunning(undefined) });
        return;
    }
  };

  const header = (
    <View style={{ paddingTop: insets.top + space[1], paddingHorizontal: space[2], paddingBottom: space[2], flexDirection: "row", alignItems: "center", gap: space[1], borderBottomWidth: 1, borderBottomColor: colors.border, backgroundColor: colors.background }}>
      <IconButton accessibilityLabel="Back" onPress={() => router.back()} icon={({ color }) => <LineIcon name="back" color={color} size={22} />} />
      <Text variant="headline" accessibilityRole="header" style={{ flex: 1 }} numberOfLines={1}>
        Farm connectors
      </Text>
    </View>
  );

  let body;
  if (list.isPending || garden.isPending) {
    body = (
      <View style={{ gap: space[3] }}>
        <Skeleton height={120} />
        <Skeleton height={120} />
      </View>
    );
  } else if (list.isError) {
    body = <ErrorState message="The farm connectors didn't load." onRetry={() => void list.refetch()} />;
  } else if (!list.data.available) {
    body = (
      <EmptyState
        title="Farm connectors are off"
        message="This PDM server isn't set up to connect Slack or Google Drive. Whoever runs it can turn them on."
      />
    );
  } else {
    const connectors = list.data.connectors;
    const connecting = flow.phase.step !== "idle" ? flow.phase.kind : undefined;
    body = (
      <>
        <Text variant="body" color="textMuted">
          Connect the farm&apos;s Slack and Google Drive, and chat and plans can use the crew&apos;s reports and the farm&apos;s
          records. PDM only reads them: it never posts or edits.
        </Text>
        {flow.phase.step === "idle" && flow.phase.error ? <Banner tone="danger" title="Not connected" message={flow.phase.error} /> : null}
        {notice ? <Banner tone={notice.tone} title={notice.title} message={notice.message} /> : null}
        {connectors.length === 0 ? (
          <EmptyState
            title="No farm connectors yet"
            message={owner ? "Connect Slack or Google Drive below." : "An owner of this garden can connect the farm's Slack or Google Drive."}
          />
        ) : (
          <Section title="Connected">
            {connectors.map((c) => (
              <ConnectorCard
                key={c.id}
                connector={c}
                view={connectorView(c, role)}
                timeZone={timeZone}
                onAction={onAction}
                busy={running?.id === c.id ? running.action : connecting === c.kind && c.status === "reauth_needed" ? "reconnect" : undefined}
              />
            ))}
          </Section>
        )}
        {owner ? (
          <Section title="Connect">
            <Text variant="footnote" color="textMuted">
              {VENDOR.slack} or {VENDOR.google_drive} opens to ask for read-only access, then you choose the channels or the folder PDM reads.
              Every member of this garden can then ask about them in chat.
            </Text>
            <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
              {KINDS.map((kind) => (
                <Button
                  key={kind}
                  label={`Connect ${VENDOR[kind]}`}
                  variant="secondary"
                  busy={connecting === kind}
                  disabled={flow.busy && connecting !== kind}
                  onPress={() => {
                    setNotice(undefined);
                    void flow.start(kind);
                  }}
                />
              ))}
            </View>
          </Section>
        ) : (
          <Text variant="footnote" color="textMuted">
            Only the garden&apos;s owners connect, choose the sources, sync or disconnect.
          </Text>
        )}
      </>
    );
  }

  return (
    <View style={{ flex: 1, backgroundColor: colors.background }} testID="connectors-screen">
      {header}
      <Screen underHeader refreshControl={<RefreshControl refreshing={list.isRefetching} onRefresh={() => void list.refetch()} />}>
        <View style={{ gap: space[5], paddingTop: space[2] }}>{body}</View>
      </Screen>
      {picker ? <SourcePicker connector={picker} onClose={() => setPicker(undefined)} /> : null}
    </View>
  );
}
