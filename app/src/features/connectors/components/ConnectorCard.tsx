// One farm connector on the Farm connectors screen (EON E10.1): its status, account, sources, last sync and record
// count, and the owner's actions. A solid card with neutral controls (design 3.1); an `era` connector carries the
// synthetic-data label chat's record sheet uses (EON decision 10) and no actions.
import { View } from "react-native";

import { formatDateTime } from "@/domain/format";
import type { Connector } from "@/domain/types";
import { Banner, Button, Card, Chip, Text, space } from "@/ui";

import { ERA_LABEL, type ConnectorAction, type ConnectorView } from "../model/view";

export interface ConnectorCardProps {
  connector: Connector;
  view: ConnectorView;
  timeZone: string;
  onAction: (action: ConnectorAction, connector: Connector) => void;
  /** The action running now, shown busy; the others wait. */
  busy?: ConnectorAction;
}

const LABEL: Record<Exclude<ConnectorAction, "choose">, string> = {
  sync: "Sync now",
  reconnect: "Reconnect",
  disconnect: "Disconnect",
  cancel: "Cancel connect",
};

export function ConnectorCard({ connector, view, timeZone, onAction, busy }: ConnectorCardProps) {
  const synced = connector.synced_at ? `Last synced ${formatDateTime(connector.synced_at, timeZone)}` : "Not synced yet";
  return (
    <Card testID={`connector-${connector.id}`} style={{ gap: space[3] }}>
      <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: space[3] }}>
        <Text variant="headline" accessibilityRole="header" style={{ flexShrink: 1 }}>
          {view.title}
        </Text>
        <Chip label={view.status} fill={connector.status === "reauth_needed" ? "watchSoft" : "surfaceMuted"} />
      </View>
      {view.era ? <Banner tone="info" title={ERA_LABEL} message="Seeded for the demo garden from Era's simulator, not a real farm. It can't be changed here." /> : null}
      <View style={{ gap: space[1] }}>
        {view.account ? <Text variant="subheadline">{view.account}</Text> : null}
        {view.sources ? (
          <Text variant="subheadline" color="textMuted">
            {view.sources}
          </Text>
        ) : null}
        {connector.status === "active" || connector.status === "reauth_needed" ? (
          <Text variant="footnote" color="textMuted">
            {`${synced} · ${view.records}`}
          </Text>
        ) : null}
        {view.note ? (
          <Text variant="footnote" color="textMuted">
            {view.note}
          </Text>
        ) : null}
      </View>
      {view.actions.length > 0 ? (
        <View style={{ flexDirection: "row", flexWrap: "wrap", gap: space[2] }}>
          {view.actions.map((a) => (
            <Button
              key={a}
              label={a === "choose" ? view.chooseLabel : LABEL[a]}
              variant={a === "disconnect" || a === "cancel" ? "plain" : "secondary"}
              size="small"
              busy={busy === a}
              disabled={busy !== undefined && busy !== a}
              accessibilityHint={a === "disconnect" ? "Asks before it deletes the synced records" : undefined}
              onPress={() => onAction(a, connector)}
            />
          ))}
        </View>
      ) : null}
    </Card>
  );
}
