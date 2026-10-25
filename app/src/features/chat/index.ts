// The chat feature's public surface (README 5): the Chat tab, the conversation, and the hook other features use
// to open a conversation about a scan, plant or garden ("Ask about this result").
export { ConversationListScreen } from "./screens/ConversationListScreen";
export { ConversationScreen } from "./screens/ConversationScreen";
export { useAskAbout } from "./hooks/useAskAbout";
// Farm citations (EON E10.3), shared with the plan: what a tap does, and the sheet an `era` record opens in.
export { FarmRecordSheet } from "./components/FarmRecordSheet";
export { isVendorRecord, openFarmCitation, SYSTEM_WORD, type FarmOpen } from "./model/farm";
