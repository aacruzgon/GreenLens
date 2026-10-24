// Mock chat (docs/frontend/README.md 15, "a scripted chat turn"): conversations, scripted turns streamed as SSE,
// confirmation cards, chat memories, message feedback and the garden's demo farm connector. A turn is chosen from
// the message's words, so the demo questions of design 5 each get the tool chips, text, answer block, citation,
// card or memory they show. Answer blocks and a farm record's text are proposed contracts (domain/proposed.ts).

import type { AnswerBlock, BlockPlant, ChatMessage, FrostPlant } from "@/domain/proposed";

import { SPECIES_FROST } from "./fixtures";
import { scenario, type ChatFault } from "./scenario";
import type { MockStore } from "./store";
import { mockImageUrl } from "./images";
import { emptyResponse, errorResponse, jsonResponse, nowTs, paginate, type DateKit, type S } from "./util";

// ---- state

export interface StoredConversation {
  conversation: S["Conversation"];
  /** Oldest first. */
  messages: ChatMessage[];
}

/** The chat half of the store, reset with it. */
export class ChatState {
  conversations = new Map<string, StoredConversation>();
  pending = new Map<string, S["PendingAction"]>();
  /** A confirmed `revise_plan` card → the plan version it stored, for a retried confirm. */
  revisions = new Map<string, S["ActionPlan"]>();
  /** Idempotency key → the completed assistant message, the replay of a retried send (contracts 1). */
  replays = new Map<string, ChatMessage>();
  /** Keys whose first attempt lost its response (`chatFault: "in_progress"`): the next try hears `409 request_in_progress`. */
  inProgress = new Set<string>();
  feedback = new Map<string, S["MessageFeedbackRequest"]>();
  /** Uploaded chat images by id, with the conversation they belong to. */
  images = new Map<string, { conversationId: string; url: string }>();
  connectors: S["Connector"][] = [];
}

export const ERA_SLACK = "conn_era_slack";
export const ERA_DRIVE = "conn_era_drive";

/** Farm records the demo garden's `era` connectors hold, with the text the record sheet shows. */
function farmRecords(k: DateKit): Record<"spray" | "field", S["FarmCitation"]> {
  return {
    spray: {
      record_id: `${ERA_DRIVE}.1.spray-log-2026`,
      system: "google_drive",
      title: "Spray log 2026",
      at: k.ts(9, 16, 20),
      link: "http://era-drive:8080/file/spray-log-2026",
      text: "Jul 14: east bed, copper on the tomatoes after the rain. Aug 2: east bed again, after leaf spots on Tomato 2. Wear gloves; keep the label with the log.",
    },
    field: {
      record_id: `${ERA_SLACK}.1.1729500000.000100`,
      system: "slack",
      title: "#field-reports",
      author: "Priya",
      at: k.ts(3, 18, 5),
      link: "http://era-slack:8080/archives/C0FIELD/p1729500000000100",
      text: "Lower leaves on both tomatoes in the east bed have brown rings again. Pulled the worst ones and bagged them, not composted.",
    },
  };
}

export function seedChat(state: ChatState, k: DateKit, gardenId: string, ids: { tomato1: string; tomato2: string }): void {
  state.connectors = [
    {
      id: ERA_SLACK,
      garden_id: gardenId,
      kind: "slack",
      endpoint: "era",
      status: "active",
      record_count: 91,
      sources: [{ id: "C0FIELD", name: "#field-reports" }, { id: "C0CREW", name: "#crew" }],
      created_at: k.ts(5),
      synced_at: k.ts(0, 7, 45),
    },
    {
      id: ERA_DRIVE,
      garden_id: gardenId,
      kind: "google_drive",
      endpoint: "era",
      status: "active",
      record_count: 5,
      sources: [{ id: "folder_farm", name: "Farm records" }],
      created_at: k.ts(5),
      synced_at: k.ts(0, 7, 45),
    },
  ];
  const records = farmRecords(k);
  const garden: StoredConversation = {
    conversation: {
      id: "conv_garden_spray",
      garden_id: gardenId,
      anchor: { type: "garden", id: gardenId },
      title: "When did we last spray?",
      created_at: k.ts(2, 17, 0),
      last_message_at: k.ts(2, 17, 1),
    },
    messages: [
      userMessage("msg_seed_1", "When did we last spray the east bed?", k.ts(2, 17, 0)),
      {
        id: "msg_seed_2",
        role: "assistant",
        text: "The spray log says the east bed was last sprayed on Aug 2, after leaf spots on Tomato 2. The crew reported brown rings on the lower tomato leaves again this week.",
        attachments: [],
        citations: [],
        farm_citations: [records.spray, records.field],
        pending_actions: [],
        interrupted: false,
        created_at: k.ts(2, 17, 1),
      },
    ],
  };
  const plant: StoredConversation = {
    conversation: {
      id: "conv_plant_tomato1",
      garden_id: gardenId,
      anchor: { type: "plant", id: ids.tomato1 },
      title: "Is Tomato 1 still growing?",
      created_at: k.ts(6, 19, 0),
      last_message_at: k.ts(6, 19, 1),
    },
    messages: [
      userMessage("msg_seed_3", "Is Tomato 1 still growing?", k.ts(6, 19, 0)),
      {
        id: "msg_seed_4",
        role: "assistant",
        text: "Yes. Its measured height went up at every scan since late September, though more slowly in the last week.",
        attachments: [],
        citations: [],
        pending_actions: [],
        interrupted: false,
        created_at: k.ts(6, 19, 1),
      },
    ],
  };
  state.conversations = new Map([garden, plant].map((c) => [c.conversation.id, c]));
}

function userMessage(id: string, text: string, at: string, attachments: S["Attachment"][] = []): ChatMessage {
  return { id, role: "user", text, attachments, citations: [], pending_actions: [], interrupted: false, created_at: at };
}

// ---- scripted turns

type Frame = { event: string; data: Record<string, unknown> };
type TurnStep = { delay: number; frame: Frame };

interface TurnCtx {
  store: MockStore;
  stored: StoredConversation;
  text: string;
  canWrite: boolean;
  messageId: string;
  /** The message came with a chat photo. */
  withPhoto: boolean;
}

const KNOWLEDGE: Record<"water" | "frost" | "spacing", S["Citation"]> = {
  water: { source_id: "omafra-vegetable-watering", publisher: "OMAFRA", title: "Watering vegetable gardens", url: "https://www.ontario.ca/page/vegetable-production" },
  frost: { source_id: "umn-ext-frost", publisher: "University of Minnesota Extension", title: "Protecting plants from frost", url: "https://extension.umn.edu/planting-and-growing-guides/protecting-plants-frost" },
  spacing: { source_id: "umn-ext-tomato-spacing", publisher: "University of Minnesota Extension", title: "Growing tomatoes in the home garden", url: "https://extension.umn.edu/vegetables/growing-tomatoes" },
};


function tool(id: string, name: string, label: string): TurnStep[] {
  return [
    { delay: 250, frame: { event: "tool_started", data: { call_id: id, tool: name, label } } },
    { delay: 700, frame: { event: "tool_finished", data: { call_id: id, ok: true } } },
  ];
}

/** The answer's text as deltas of a few words, as a model streams it. */
function say(text: string): TurnStep[] {
  const words = text.split(/(?<= )/);
  const out: TurnStep[] = [];
  for (let i = 0; i < words.length; i += 3) out.push({ delay: 45, frame: { event: "text_delta", data: { text: words.slice(i, i + 3).join("") } } });
  return out;
}

const block = (b: AnswerBlock): TurnStep => ({ delay: 200, frame: { event: "answer_block", data: { block: b } } });
const cite = (c: S["Citation"]): TurnStep => ({ delay: 60, frame: { event: "citation", data: { citation: c } } });

function blockPlants(store: MockStore): BlockPlant[] {
  return [...store.plants.values()].map((p) => {
    const height = store.pointsOf(p.id, "plant_height_cm").at(-1)?.value;
    return { plant_id: p.id, name: p.name, status: store.plantView(p).status, ...(height !== undefined ? { height_cm: height } : {}) };
  });
}

function anchorPlant(ctx: TurnCtx): string | undefined {
  const a = ctx.stored.conversation.anchor;
  if (a.type === "plant") return a.id;
  if (a.type === "scan") return ctx.store.scan(a.id)?.scan.plant_id;
  return undefined;
}

function plantNamed(store: MockStore, text: string): S["Plant"] | undefined {
  const lower = text.toLowerCase();
  const all = [...store.plants.values()].sort((a, b) => b.name.length - a.name.length);
  const p = all.find((x) => lower.includes(x.name.toLowerCase()));
  return p ? store.plantView(p) : undefined;
}

function pendingAction(ctx: TurnCtx, card: S["Card"], task: Omit<S["Task"], "id" | "garden_id" | "source" | "created_at">): S["PendingAction"] {
  const store = ctx.store;
  const conv = ctx.stored.conversation;
  const action: S["PendingAction"] = {
    id: store.nextId("pa"),
    conversation_id: conv.id,
    tool: "create_task",
    status: "proposed",
    card,
    preview: { kind: "task", task: { id: store.nextId("task_preview"), garden_id: conv.garden_id, source: "chat", created_at: nowTs(), ...task } },
    expires_at: nowTs(24 * 3_600_000),
  };
  store.chat.pending.set(action.id, action);
  return action;
}

function frostNights(store: MockStore): S["ForecastDay"][] {
  return store.weather.forecast.filter((d) => d.frost_risk);
}

function weekday(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y!, m! - 1, d!).toLocaleDateString("en-CA", { weekday: "long" });
}

function frostTurn(ctx: TurnCtx): TurnStep[] {
  const night = frostNights(ctx.store)[0];
  // The garden's live plants, not the map's pins (S5), with their species' frost.
  const plants: FrostPlant[] = blockPlants(ctx.store).map((p) => {
    const frost = SPECIES_FROST[ctx.store.plants.get(p.plant_id)?.species?.id ?? ""];
    return frost ? { ...p, frost_tender: frost === "tender" } : p;
  });
  const tender = plants.filter((p) => p.frost_tender === true);
  // No frost night: the API refuses the block, and the text stands alone.
  if (!night) {
    return [...tool("call_fc", "get_forecast", "Checking the forecast"), ...say("No frost is forecast in the next 7 days, so nothing needs covering this week.")];
  }
  const names = tender.map((p) => p.name);
  const list = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names[0] ?? "none";
  return [
    ...tool("call_fc", "get_forecast", "Checking the forecast"),
    ...tool("call_sp", "get_species_profile", "Checking which plants are tender"),
    ...say(`Frost is forecast on ${weekday(night.date)} night, with a low of ${night.t_min_c} °C. ${tender.length} of your plants are tender to frost: ${list}. Choose which to cover below, and add them to your tasks for that evening.`),
    block({ kind: "frost_planner", call_id: "call_fc", nights: frostNights(ctx.store), plants }),
    cite(KNOWLEDGE.frost),
  ];
}

/** "Add a task to cover Tomato 1 and Basil on Thursday evening", typed: the agent's `create_task` card. */
function coverTaskTurn(ctx: TurnCtx): TurnStep[] {
  const night = frostNights(ctx.store)[0];
  const chosen = [...ctx.store.plants.values()].filter((p) => ctx.text.toLowerCase().includes(p.name.toLowerCase()));
  if (!ctx.canWrite) {
    return say("Only editors and owners can add tasks in this garden. Ask one of them to add it, or cover the plants yourself on the evening before the frost.");
  }
  const names = chosen.map((p) => p.name);
  const list = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names[0] ?? "the tender plants";
  const action = pendingAction(
    ctx,
    { title: "Add a task?", body: `Cover ${list} before the frost`, confirm_label: "Add task", reject_label: "Not now" },
    { title: `Cover ${list}`, detail: "Throw a sheet or a frost cloth over them in the evening, and take it off in the morning.", ...(night ? { due: night.date } : {}) },
  );
  return [
    ...tool("call_task", "create_task", "Preparing a task"),
    ...say("Here is the task. It is added only when you confirm."),
    { delay: 150, frame: { event: "pending_action", data: { pending_action: action } } },
  ];
}

/** The scan whose plan a revision changes: the anchor's, else the plant's or Tomato 1's latest planned scan. */
function planScan(ctx: TurnCtx) {
  const a = ctx.stored.conversation.anchor;
  if (a.type === "scan") return ctx.store.scan(a.id);
  const plantId = anchorPlant(ctx) ?? plantNamed(ctx.store, ctx.text)?.id ?? [...ctx.store.plants.keys()][0];
  return [...ctx.store.scans.values()]
    .filter((st) => st.scan.plant_id === plantId && st.plan)
    .sort((x, y) => y.scan.created_at.localeCompare(x.scan.created_at))[0];
}

/** "No copper, please": a `revise_plan` card, which a confirm turns into a new plan version (contracts 8). */
function planTurn(ctx: TurnCtx): TurnStep[] {
  const st = planScan(ctx);
  const plan = st?.plan;
  if (!plan) return say("There's no plan to change yet. Once a scan has a plan, I can change it with you.");
  if (!ctx.canWrite) return say("Only editors and owners can change the plan. Ask one of them, and they can change it here.");
  const chemical = plan.steps.find((x) => x.category === "chemical") ?? plan.steps.at(-1)!;
  const changed: S["PlanStep"] = {
    ...chemical,
    title: "Remove the spotted lower leaves each week instead of spraying",
    detail: "Cut them off at the stem on a dry day and bin them, not the compost. No copper products.",
    category: "mechanical",
  };
  delete changed.safety_note;
  const conv = ctx.stored.conversation;
  const action: S["PendingAction"] = {
    id: ctx.store.nextId("pa"),
    conversation_id: conv.id,
    tool: "revise_plan",
    status: "proposed",
    card: { title: "Change the plan?", body: "Replace the copper spray with removing the spotted leaves each week, and keep copper out of later versions.", confirm_label: "Change plan", reject_label: "Keep plan" },
    preview: { kind: "plan", base_plan_id: plan.id, plan_summary: plan.summary, changed_steps: [changed], removed_step_ids: [], constraints_added: ["No copper products"] },
    expires_at: nowTs(24 * 3_600_000),
  };
  ctx.store.chat.pending.set(action.id, action);
  return [
    ...tool("call_plan", "revise_plan", "Preparing a plan change"),
    ...say("Here is the change. The plan changes only when you confirm, and it changes for everyone in the garden."),
    { delay: 150, frame: { event: "pending_action", data: { pending_action: action } } },
  ];
}

function waterTurn(ctx: TurnCtx): TurnStep[] {
  const days = ctx.store.weather.forecast.slice(0, 5);
  const rainy = days.find((d) => d.precip_mm >= 2);
  const steps: TurnStep[] = [
    ...tool("call_fc", "get_forecast", "Checking the forecast"),
    ...tool("call_soil", "get_soil_profile", "Checking the soil"),
    ...say(
      rainy
        ? `Rain is forecast on ${weekday(rainy.date)}, and the soil is still moist at 9 cm. Skip watering today and check the soil again before then.`
        : "No rain is forecast this week, and the top of the soil is drying. Water at the base in the morning, and check the soil a finger deep first.",
    ),
    block({ kind: "weather_hours", call_id: "call_fc", days }),
    cite(KNOWLEDGE.water),
  ];
  if (ctx.canWrite) {
    const action = pendingAction(
      ctx,
      { title: "Add a task?", body: "Check the soil a finger deep before watering", confirm_label: "Add task", reject_label: "Not now" },
      { title: "Check the soil before watering", ...(days[2] ? { due: days[2].date } : {}) },
    );
    steps.push({ delay: 150, frame: { event: "pending_action", data: { pending_action: action } } });
  }
  return steps;
}

function memoryTurn(ctx: TurnCtx): TurnStep[] {
  const plantId = anchorPlant(ctx) ?? plantNamed(ctx.store, ctx.text)?.id;
  const plant = plantId ? ctx.store.plants.get(plantId) : undefined;
  const memory: S["Memory"] = {
    id: ctx.store.nextId("mem"),
    garden_id: ctx.stored.conversation.garden_id,
    kind: "fact",
    topic: "container",
    source: "chat",
    shared: false,
    text: plant ? `${plant.name} is in a container` : "The plants are in containers",
    created_at: nowTs(),
    ...(plant ? { plant_id: plant.id } : {}),
  };
  ctx.store.memories.push(memory);
  return [
    ...say("Thanks, that helps. Containers dry out faster than beds, so check them a day sooner after warm weather."),
    { delay: 300, frame: { event: "memory_saved", data: { memory } } },
  ];
}

function spacingTurn(ctx: TurnCtx): TurnStep[] {
  const map = ctx.store.maps.get(ctx.stored.conversation.garden_id);
  if (!map) return [...tool("call_map", "get_garden_map", "Reading the garden map"), ...say("Your garden has no map yet, so I can't measure the spacing. Map the beds from the garden screen first.")];
  const focusId = anchorPlant(ctx) ?? plantNamed(ctx.store, ctx.text)?.id ?? map.pins[0]!.plant_id;
  const me = map.pins.find((p) => p.plant_id === focusId) ?? map.pins[0]!;
  const near = map.pins
    .filter((p) => p.plant_id !== me.plant_id)
    .map((p) => ({ p, d: Math.hypot(p.x_m - me.x_m, p.y_m - me.y_m) }))
    .sort((a, b) => a.d - b.d)[0]!;
  const cm = Math.round((near.d * 100) / 5) * 5;
  const a = ctx.store.plants.get(me.plant_id)!.name;
  const b = ctx.store.plants.get(near.p.plant_id)!.name;
  const neighbours: Record<string, string[]> = Object.fromEntries(
    map.pins.map((p) => [p.plant_id, map.pins.filter((q) => q !== p && Math.hypot(q.x_m - p.x_m, q.y_m - p.y_m) < 0.6).map((q) => q.plant_id)]),
  );
  return [
    ...tool("call_map", "get_garden_map", "Reading the garden map"),
    ...tool("call_m", "get_plant_measurements", "Reading the measurements"),
    ...say(`${a} is ${cm} cm from ${b}, the closest plant on your map. Tomatoes do best about 60 cm apart, so their leaves stay dry and a leaf spot spreads more slowly.`),
    block({ kind: "garden_map", call_id: "call_map", map, plants: blockPlants(ctx.store), focus_plant_ids: [me.plant_id, near.p.plant_id], neighbours }),
    cite(KNOWLEDGE.spacing),
  ];
}

function trendTurn(ctx: TurnCtx): TurnStep[] {
  const plant = plantNamed(ctx.store, ctx.text) ?? (anchorPlant(ctx) ? ctx.store.plantView(ctx.store.plants.get(anchorPlant(ctx)!)!) : undefined) ?? ctx.store.plantView([...ctx.store.plants.values()][0]!);
  const points = ctx.store.pointsOf(plant.id, "plant_height_cm");
  const first = points[0];
  const last = points.at(-1);
  return [
    ...tool("call_m", "get_plant_measurements", "Reading the measurements"),
    ...say(
      first && last && points.length >= 3
        ? `${plant.name} went from about ${first.value} cm to about ${last.value} cm over ${points.length} scans. It is still growing, more slowly in the last week.`
        : `${plant.name} has too few measured scans for a trend yet. Each scan with the whole plant in view adds one.`,
    ),
    block({ kind: "measurement_trend", call_id: "call_m", plant_id: plant.id, plant_name: plant.name, quantity: "plant_height_cm", points }),
  ];
}

function lookalikeTurn(): TurnStep[] {
  return [
    ...tool("call_cl", "compare_lookalikes", "Comparing look-alikes"),
    ...say("Early blight and Septoria leaf spot both start on the lower leaves. The spots tell them apart: look at one closely in good light."),
    block({
      kind: "candidates",
      call_id: "call_cl",
      disease: { problem_id: "tomato_early_blight", name: "Early blight" },
      lookalikes: [
        { problem_id: "tomato_septoria_leaf_spot", name: "Septoria leaf spot", how_to_tell: "Many small spots, 2–3 mm, with grey centres and tiny black dots; no rings." },
      ],
    }),
  ];
}

function farmTurn(ctx: TurnCtx): { steps: TurnStep[]; farm: S["FarmCitation"][] } {
  const records = farmRecords(ctx.store.dates);
  return {
    steps: [
      ...tool("call_farm", "search_farm_records", "Searching the farm records"),
      ...say("The spray log has the east bed last sprayed on Aug 2, after leaf spots on Tomato 2. In #field-reports this week, Priya reported brown rings on the lower tomato leaves again and removed the worst ones."),
    ],
    farm: [records.spray, records.field],
  };
}

function rescanTurn(ctx: TurnCtx): TurnStep[] {
  const plantId = anchorPlant(ctx) ?? plantNamed(ctx.store, ctx.text)?.id ?? [...ctx.store.plants.keys()][1]!;
  const instructions = "Take a close-up of one spotted leaf, about 30 cm away, in daylight.";
  return [
    ...tool("call_rs", "request_rescan", "Asking for a photo"),
    ...say("A closer photo would settle it. Take one of a spotted leaf and I'll compare it with the last scan."),
    { delay: 200, frame: { event: "rescan_requested", data: { plant_id: plantId, photo_kind: "leaf_closeup", instructions } } },
  ];
}

function defaultTurn(withPhoto: boolean): TurnStep[] {
  if (withPhoto) {
    return [
      ...tool("call_img", "view_image", "Looking at your photo"),
      ...say("I've looked at your photo beside the last scan. Ask what you'd like to know: whether the spots are spreading, or what to do next."),
    ];
  }
  return [
    ...tool("call_kb", "search_knowledge", "Searching the guides"),
    ...say("I can help with watering, frost, spacing, growth and what the last scan found. Ask about one of your plants, or attach a photo."),
  ];
}

/** The turn for a message, and the farm records it checked (those arrive on the stored message only). */
function turnFor(ctx: TurnCtx): { route: S["ChatRoute"]; steps: TurnStep[]; farm: S["FarmCitation"][] } {
  const t = ctx.text.toLowerCase();
  const has = (...words: string[]) => words.some((w) => t.includes(w));
  if (has("task to cover", "add a task to cover")) return { route: "task_change", steps: coverTaskTurn(ctx), farm: [] };
  if (has("copper", "organic", "change the plan")) return { route: "task_change", steps: planTurn(ctx), farm: [] };
  if (has("frost", "cover", "cold")) return { route: "care_question", steps: frostTurn(ctx), farm: [] };
  if (has("container", "in a pot", "remember")) return { route: "care_question", steps: memoryTurn(ctx), farm: [] };
  if (has("water", "rain")) return { route: "care_question", steps: waterTurn(ctx), farm: [] };
  if (has("spray", "report", "field", "log", "crew", "records")) {
    const f = farmTurn(ctx);
    return { route: "care_question", steps: f.steps, farm: f.farm };
  }
  if (has("photo", "picture", "closer", "rescan")) return { route: "diagnosis_question", steps: rescanTurn(ctx), farm: [] };
  if (has("spacing", "space", "crowd", "close", "apart", "map")) return { route: "care_question", steps: spacingTurn(ctx), farm: [] };
  if (has("grow", "height", "tall", "trend")) return { route: "care_question", steps: trendTurn(ctx), farm: [] };
  if (has("sure", "septoria", "tell", "lookalike", "look-alike", "which one")) return { route: "diagnosis_question", steps: lookalikeTurn(), farm: [] };
  return { route: "care_question", steps: defaultTurn(ctx.withPhoto), farm: [] };
}

// ---- the stream

const encoder = new TextEncoder();

function frameText(f: Frame): string {
  return `event: ${f.event}\ndata: ${JSON.stringify({ v: 1, ...f.data })}\n\n`;
}

/** The assistant message the events build, stored when the turn ends (complete or interrupted). */
function assemble(id: string, frames: Frame[], farm: S["FarmCitation"][], interrupted: boolean): ChatMessage {
  const msg: ChatMessage = { id, role: "assistant", text: "", attachments: [], citations: [], pending_actions: [], interrupted, created_at: nowTs() };
  for (const f of frames) {
    if (f.event === "text_delta") msg.text += f.data.text as string;
    if (f.event === "citation") msg.citations.push(f.data.citation as S["Citation"]);
    if (f.event === "pending_action") msg.pending_actions.push(f.data.pending_action as S["PendingAction"]);
    if (f.event === "answer_block") (msg.blocks ??= []).push(f.data.block as AnswerBlock);
  }
  if (farm.length && !interrupted) msg.farm_citations = farm;
  return msg;
}

function turnStream(store: MockStore, stored: StoredConversation, body: S["SendMessageRequest"], key: string | null, fault: ChatFault | null = null): ReadableStream<Uint8Array> {
  const gen = store.generation;
  const drop = scenario.get().dropNextStream;
  if (drop) scenario.set({ dropNextStream: false });
  const messageId = store.nextId("msg");
  const role = scenario.get().role;
  const ctx: TurnCtx = { store, stored, text: body.text, canWrite: role === "editor" || role === "owner", messageId, withPhoto: (body.image_ids?.length ?? 0) > 0 };
  const turn = turnFor(ctx);
  const sent: Frame[] = [];
  let cancelled = false;
  let finished = false;
  let unstall: (() => void) | undefined;

  const finish = (interrupted: boolean) => {
    if (finished || store.generation !== gen) return;
    finished = true;
    const msg = assemble(messageId, sent, turn.farm, interrupted);
    stored.messages.push(msg);
    stored.conversation.last_message_at = msg.created_at;
    if (!interrupted && key) store.chat.replays.set(key, msg);
  };

  async function pump(controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> {
    const scale = scenario.get().delayScale;
    const steps: TurnStep[] = [
      { delay: 150, frame: { event: "message_started", data: { message_id: messageId, route: turn.route } } },
      ...turn.steps,
    ];
    try {
      let count = 0;
      for (const step of steps) {
        if (scale > 0) await store.sleep(step.delay * scale);
        if (cancelled || store.generation !== gen) return finish(true);
        controller.enqueue(encoder.encode(frameText(step.frame)));
        sent.push(step.frame);
        count += 1;
        if (drop && count >= 4) {
          finish(true);
          await store.sleep(5); // let the reader take the frames already queued
          controller.error(new TypeError("Network request failed"));
          return;
        }
        if (fault === "error_event" && count >= 4) {
          // The server ends the turn with an `error` event and stores it as interrupted (contracts 8).
          const error: Frame = { event: "error", data: { code: "upstream_unavailable", message: "The assistant stopped partway. Try again in a minute.", retryable: true } };
          controller.enqueue(encoder.encode(frameText(error)));
          finish(true);
          controller.close();
          return;
        }
        if (fault === "stall" && count >= 3) {
          // Quiet, with no keepalives, until the app gives up and closes it.
          await new Promise<void>((resolve) => (unstall = resolve));
          return finish(true);
        }
      }
      finish(false);
      controller.enqueue(encoder.encode(frameText({ event: "message_completed", data: { message_id: messageId, usage: { tokens_in: 1840, tokens_out: 160 } } })));
      controller.close();
    } catch {
      finish(true); // The reader went away.
    }
  }

  return new ReadableStream<Uint8Array>({
    start(controller) {
      void pump(controller);
    },
    cancel() {
      cancelled = true;
      finish(true);
      unstall?.();
    },
  });
}

// ---- routes

/**
 * A confirmed plan revision: a new version of the scan's plan with the changed steps, the removed ones gone and the
 * constraints added, superseding the base (`plan_superseded`, reason `chat_revision`). A base that is no longer
 * current is `409 invalid_state` with `current_plan_id`, which the card shows as "Ask again".
 */
function confirmPlan(store: MockStore, action: S["PendingAction"], preview: Extract<S["Preview"], { kind: "plan" }>): Response {
  const st = [...store.scans.values()].find((x) => x.plan && (x.plan.id === preview.base_plan_id || x.plan.supersedes === preview.base_plan_id || x.scan.plan_id === preview.base_plan_id));
  const base = st?.plan;
  if (!st || !base) return errorResponse("invalid_state", { status: action.status });
  if (base.id !== preview.base_plan_id) return errorResponse("invalid_state", { status: action.status, current_plan_id: base.id });
  const changed = new Map(preview.changed_steps.map((x) => [x.id, x]));
  const steps = base.steps.filter((x) => !preview.removed_step_ids.includes(x.id)).map((x) => changed.get(x.id) ?? x);
  for (const x of preview.changed_steps) if (!base.steps.some((b) => b.id === x.id)) steps.push(x);
  const at = nowTs();
  const plan: S["ActionPlan"] = {
    ...base,
    id: store.nextId("plan"),
    version: base.version + 1,
    supersedes: base.id,
    summary: preview.plan_summary,
    steps,
    constraints: [...base.constraints, ...preview.constraints_added.map((text) => ({ text, from_pending_action_id: action.id, created_at: at }))],
    created_at: at,
  };
  st.plan = plan;
  st.scan.plan_id = plan.id;
  store.emit(st.scan.id, { type: "plan_superseded", old_plan_id: base.id, new_plan_id: plan.id, reason: "chat_revision" });
  action.status = "confirmed";
  store.chat.revisions.set(action.id, plan);
  return jsonResponse(200, { pending_action: action, result: { plan } });
}

export interface ChatRouteCtx {
  params: Record<string, string>;
  query: URLSearchParams;
  headers: Headers;
  store: MockStore;
  json(): Promise<unknown>;
  form(): Promise<{ fileType?: string }>;
}

type Register = (
  method: string,
  pattern: string,
  handler: (c: ChatRouteCtx) => Response | Promise<Response>,
  minRole?: S["Role"] | boolean,
) => void;

export interface ChatRouteHelpers {
  jsonObject(c: ChatRouteCtx): Promise<Record<string, unknown>>;
  streamResponse(stream: ReadableStream<Uint8Array>): Response;
}

const MAX_TEXT = 2000;
const ACCEPTED = ["image/jpeg", "image/png", "image/webp", "image/heic"];

function view(store: MockStore, stored: StoredConversation, query: URLSearchParams): S["ConversationDetail"] {
  // Newest first, paged (contracts 8).
  const page = paginate([...stored.messages].reverse(), query);
  return { ...stored.conversation, messages: page as S["ConversationDetail"]["messages"] };
}

export function registerChatRoutes(route: Register, h: ChatRouteHelpers): void {
  const invalid = (field: string, problem: string) => errorResponse("invalid_request", { field, problem });
  const notFound = () => errorResponse("not_found");

  route("GET", "/conversations", (c) => {
    const gardenId = c.query.get("garden_id");
    if (!gardenId) return invalid("garden_id", "required");
    if (!c.store.gardens.has(gardenId)) return notFound();
    const type = c.query.get("anchor_type");
    const id = c.query.get("anchor_id");
    const items = [...c.store.chat.conversations.values()]
      .map((s) => s.conversation)
      .filter((x) => x.garden_id === gardenId && (!type || x.anchor.type === type) && (!id || x.anchor.id === id))
      .sort((a, b) => (b.last_message_at ?? b.created_at).localeCompare(a.last_message_at ?? a.created_at));
    return jsonResponse(200, paginate(items, c.query));
  });

  route("POST", "/conversations", async (c) => {
    const body = await h.jsonObject(c);
    const gardenId = typeof body.garden_id === "string" ? body.garden_id : "";
    if (!c.store.gardens.has(gardenId)) return notFound();
    const anchor = body.anchor as { type?: unknown; id?: unknown } | undefined;
    if (!anchor || !["scan", "plant", "garden"].includes(anchor.type as string) || typeof anchor.id !== "string") {
      return invalid("anchor", "a scan, plant or garden of the garden");
    }
    const exists =
      anchor.type === "garden" ? anchor.id === gardenId : anchor.type === "plant" ? c.store.plants.has(anchor.id) : Boolean(c.store.scan(anchor.id));
    if (!exists) return notFound();
    const key = c.headers.get("idempotency-key");
    const prior = key ? [...c.store.chat.conversations.values()].find((s) => s.conversation.id === `conv_${key}`) : undefined;
    if (prior) return jsonResponse(201, prior.conversation);
    const conversation: S["Conversation"] = {
      id: key ? `conv_${key}` : c.store.nextId("conv"),
      garden_id: gardenId,
      anchor: { type: anchor.type, id: anchor.id } as S["Anchor"],
      created_at: nowTs(),
    };
    c.store.chat.conversations.set(conversation.id, { conversation, messages: [] });
    return jsonResponse(201, conversation);
  });

  route("GET", "/conversations/:id", (c) => {
    const stored = c.store.chat.conversations.get(c.params.id);
    return stored ? jsonResponse(200, view(c.store, stored, c.query)) : notFound();
  });

  route("POST", "/conversations/:id/images", async (c) => {
    if (!c.store.chat.conversations.has(c.params.id)) return notFound();
    const fault = scenario.get().uploadFault;
    if (fault) {
      scenario.set({ uploadFault: null });
      return errorResponse(fault === "image_too_dark" ? "image_quality" : fault);
    }
    const form = await c.form();
    if (form.fileType && !ACCEPTED.includes(form.fileType)) return errorResponse("unsupported_media_type");
    const id = c.store.nextId("chatimg");
    const url = mockImageUrl("tomato-blight");
    c.store.chat.images.set(id, { conversationId: c.params.id, url });
    return jsonResponse(201, { image_id: id, url });
  });

  route("POST", "/conversations/:id/messages", async (c) => {
    const stored = c.store.chat.conversations.get(c.params.id);
    if (!stored) return notFound();
    const fault = scenario.get().chatFault;
    if (fault) scenario.set({ chatFault: null });
    if (fault === "budget_exceeded") return errorResponse("budget_exceeded", { budget: "chat_tokens", resets_at: nowTs(8 * 3_600_000) });
    const key = c.headers.get("idempotency-key");
    if (key && c.store.chat.inProgress.delete(key)) return errorResponse("request_in_progress", { status: "running" });
    const replay = key ? c.store.chat.replays.get(key) : undefined;
    if (replay) return jsonResponse(200, { message: replay });
    const body = await h.jsonObject(c);
    const text = typeof body.text === "string" ? body.text.trim() : "";
    if (!text) return invalid("text", "required");
    if (text.length > MAX_TEXT) return invalid("text", `at most ${MAX_TEXT} characters`);
    const imageIds = Array.isArray(body.image_ids) ? (body.image_ids as unknown[]) : [];
    if (imageIds.length > 3) return invalid("image_ids", "at most 3");
    const attachments: S["Attachment"][] = [];
    for (const id of imageIds) {
      const img = typeof id === "string" ? c.store.chat.images.get(id) : undefined;
      if (!img || img.conversationId !== stored.conversation.id) return invalid("image_ids", "unknown image");
      attachments.push({ image_id: id as string, url: img.url });
    }
    const user = userMessage(c.store.nextId("msg"), text, nowTs(), attachments);
    stored.messages.push(user);
    stored.conversation.title ??= text.length > 60 ? `${text.slice(0, 57)}…` : text;
    stored.conversation.last_message_at = user.created_at;
    const stream = turnStream(c.store, stored, { text, image_ids: imageIds as string[] }, key, fault);
    if (fault === "in_progress" && key) {
      // The turn runs to the end on the server, but its response never reaches the app.
      const reader = stream.getReader();
      while (!(await reader.read()).done);
      c.store.chat.inProgress.add(key);
      throw new TypeError("Network request failed");
    }
    return h.streamResponse(stream);
  });

  route("POST", "/messages/:id/feedback", async (c) => {
    const msg = [...c.store.chat.conversations.values()].flatMap((s) => s.messages).find((m) => m.id === c.params.id);
    if (!msg || msg.role !== "assistant") return notFound();
    const body = await h.jsonObject(c);
    if (body.rating !== "up" && body.rating !== "down") return invalid("rating", "up or down");
    c.store.chat.feedback.set(msg.id, { rating: body.rating, ...(typeof body.notes === "string" ? { notes: body.notes } : {}) });
    return emptyResponse(204);
  });

  route("POST", "/pending-actions/:id/confirm", (c) => {
    const action = c.store.chat.pending.get(c.params.id);
    if (!action) return notFound();
    if (action.status === "confirmed") {
      const task = action.preview.kind === "task" ? c.store.tasks.find((t) => t.id === `task_${action.id}`) : undefined;
      const plan = action.preview.kind === "plan" ? c.store.chat.revisions.get(action.id) : undefined;
      return jsonResponse(200, { pending_action: action, result: task ? { task } : plan ? { plan } : {} });
    }
    if (action.status !== "proposed" || action.expires_at < nowTs()) {
      if (action.status === "proposed") action.status = "expired";
      return errorResponse("invalid_state", { status: action.status });
    }
    if (action.preview.kind === "plan") return confirmPlan(c.store, action, action.preview);
    const preview = action.preview.task;
    if (preview.plant_id && !c.store.plants.has(preview.plant_id)) return errorResponse("invalid_state", { status: action.status, hint: "plant_removed" });
    const task: S["Task"] = { ...preview, id: `task_${action.id}`, source: "chat", created_at: nowTs() };
    c.store.tasks.push(task);
    action.status = "confirmed";
    return jsonResponse(200, { pending_action: action, result: { task } });
  }, true);

  route("POST", "/pending-actions/:id/reject", (c) => {
    const action = c.store.chat.pending.get(c.params.id);
    if (!action) return notFound();
    if (action.status === "proposed") action.status = "rejected";
    return jsonResponse(200, { pending_action: action });
  }, true);

  route("POST", "/memories/:id/share", (c) => {
    const memory = c.store.memories.find((m) => m.id === c.params.id);
    if (!memory) return notFound();
    memory.source = "user";
    memory.shared = true;
    return jsonResponse(200, memory);
  }, true);

  route("DELETE", "/memories/:id", (c) => {
    const i = c.store.memories.findIndex((m) => m.id === c.params.id);
    if (i < 0) return notFound();
    const memory = c.store.memories[i]!;
    // A chat memory: its author (the mock's one user). A shared one: an owner.
    if (memory.shared && scenario.get().role !== "owner") return errorResponse("forbidden", { required_role: "owner" });
    c.store.memories.splice(i, 1);
    return emptyResponse(204);
  });
  // The garden's connectors, which this file seeds, are served by connectors.ts.
}
