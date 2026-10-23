// Contract types the app uses, re-exported by name from the generated schema
// (docs/frontend/README.md §5, §9). Never hand-write a contract shape; add a line here.
import type { components } from "@/api/generated/schema";

type S = components["schemas"];

// Wire primitives
export type Id = S["Id"];
export type Slug = S["Slug"];
export type Timestamp = S["Timestamp"];
export type LocalDate = S["LocalDate"];
export type TimeZone = S["TimeZone"];
export type SignedUrl = S["SignedUrl"];
export type Cursor = S["Cursor"];
export type EventSeq = S["EventSeq"];
export type Run = S["Run"];
export type BBox = S["BBox"];

// Errors
export type ErrorBody = S["ErrorBody"];
export type ErrorObject = S["ErrorObject"];
export type ErrorCode = S["ErrorCode"];
export type ErrorKind = S["ErrorKind"];

// Accounts, gardens, members
export type Me = S["Me"];
export type UpdateMeRequest = S["UpdateMeRequest"];
export type UserRef = S["UserRef"];
export type Role = S["Role"];
export type GardenRole = S["Role"];
export type Garden = S["Garden"];
export type CreateGardenRequest = S["CreateGardenRequest"];
export type UpdateGardenRequest = S["UpdateGardenRequest"];
export type GardenLocationInput = S["GardenLocationInput"];
export type LocationPrecision = S["LocationPrecision"];
export type Member = S["Member"];
export type Invite = S["Invite"];
export type CreateInviteRequest = S["CreateInviteRequest"];
export type GardenTransfer = S["GardenTransfer"];
export type AccountDeletion = S["AccountDeletion"];
export type PushToken = S["PushToken"];
export type PushTokenRequest = S["PushTokenRequest"];
export type Platform = S["Platform"];

// Plants and species
export type Plant = S["Plant"];
export type PlantStatus = S["PlantStatus"];
export type CreatePlantRequest = S["CreatePlantRequest"];
export type UpdatePlantRequest = S["UpdatePlantRequest"];
export type NewPlantInput = S["NewPlantInput"];
export type PlantPosition = S["PlantPosition"];
export type LatestScan = S["LatestScan"];
export type SpeciesSummary = S["SpeciesSummary"];
export type SpeciesRef = S["SpeciesRef"];
export type SpeciesCandidate = S["SpeciesCandidate"];
export type SpeciesOption = S["SpeciesOption"];
export type SpeciesStatus = S["SpeciesStatus"];
export type ConfirmSpeciesRequest = S["ConfirmSpeciesRequest"];
export type TimelineEntry = S["TimelineEntry"];
export type TimelineMeasurements = S["TimelineMeasurements"];
export type MeasurementPoint = S["MeasurementPoint"];
export type GrowthQuantity = S["GrowthQuantity"];

// Scans
export type Scan = S["Scan"];
export type ScanDetail = S["ScanDetail"];
export type ScanStatus = S["ScanStatus"];
export type ScanImage = S["ScanImage"];
export type ImageKind = S["ImageKind"];
export type UploadImageResponse = S["UploadImageResponse"];
export type CreateScanRequest = S["CreateScanRequest"];
export type ScanFailure = S["ScanFailure"];
export type ScanFailureCode = S["ScanFailureCode"];
export type GuidedCheck = S["GuidedCheck"];
export type GuidedCheckStatus = S["GuidedCheckStatus"];
export type GuidedKind = S["GuidedKind"];
export type Question = S["Question"];
export type QuestionKind = S["QuestionKind"];
export type QuestionOption = S["QuestionOption"];
export type QuestionStatus = S["QuestionStatus"];
export type Answer = S["Answer"];
export type AnswerRequest = S["AnswerRequest"];
export type FeedbackRequest = S["FeedbackRequest"];
export type QualityReport = S["QualityReport"];
export type QualityHint = S["QualityHint"];
export type Stage = S["Stage"];
export type RunReason = S["RunReason"];
export type ProblemKind = S["ProblemKind"];

// Scan events
export type ScanEvent = S["ScanEvent"];
export type ScanEventBody = S["ScanEventBody"];
export type ScanEventType = ScanEventBody["type"];
export type ScanEventOf<T extends ScanEventType> = Extract<ScanEventBody, { type: T }>;
export type RunStartedEvent = ScanEventOf<"run_started">;
export type StatusChangedEvent = ScanEventOf<"status_changed">;
export type StageStartedEvent = ScanEventOf<"stage_started">;
export type StageCompletedEvent = ScanEventOf<"stage_completed">;
export type FindingEvent = ScanEventOf<"finding">;
export type FindingRetractedEvent = ScanEventOf<"finding_retracted">;
export type WeatherLoadedEvent = ScanEventOf<"weather_loaded">;
export type CandidatesSelectedEvent = ScanEventOf<"candidates_selected">;
export type SpeciesSuggestedEvent = ScanEventOf<"species_suggested">;
export type SpeciesConfirmedEvent = ScanEventOf<"species_confirmed">;
export type QuestionEvent = ScanEventOf<"question">;
export type QuestionResolvedEvent = ScanEventOf<"question_resolved">;
export type GuidedCheckDoneEvent = ScanEventOf<"guided_check_done">;
export type DiagnosisEvent = ScanEventOf<"diagnosis">;
export type PlanReadyEvent = ScanEventOf<"plan_ready">;
export type PlanSupersededEvent = ScanEventOf<"plan_superseded">;
export type FailedEvent = ScanEventOf<"failed">;

// Findings
export type Observation = S["Observation"];
export type ObservationDetail = S["ObservationDetail"];
export type ObservationSource = S["ObservationSource"];
export type ObservationStage = S["ObservationStage"];
export type Region = S["Region"];
export type Confidence = S["Confidence"];
export type ConfidenceBand = S["ConfidenceBand"];
export type Measurement = S["Measurement"];
export type MeasureQuantity = S["MeasureQuantity"];
export type OrganismRole = S["OrganismRole"];
export type OrganismFinding = S["OrganismFinding"];
export type SoilFinding = S["SoilFinding"];
export type DamageFinding = S["DamageFinding"];
export type SoilAspect = S["SoilAspect"];

// Diagnosis and plan
export type Diagnosis = S["Diagnosis"];
export type DiagnosisStatus = S["DiagnosisStatus"];
export type DiagnosisOrganism = S["DiagnosisOrganism"];
export type CandidateScore = S["CandidateScore"];
export type IndicatorEvidence = S["IndicatorEvidence"];
export type Risk = S["Risk"];
export type Citation = S["Citation"];
export type ActionPlan = S["ActionPlan"];
export type PlanStep = S["PlanStep"];
export type StepCategory = S["StepCategory"];
export type PlanWhen = S["PlanWhen"];
export type PlanTarget = S["PlanTarget"];
export type FarmCitation = S["FarmCitation"];
export type TriggerKind = S["TriggerKind"];
export type SupersedeReason = S["SupersedeReason"];
export type EvidenceBasis = S["EvidenceBasis"];
export type Verdict = S["Verdict"];

// Tasks, weather, memory, map
export type Task = S["Task"];
export type CreateTaskRequest = S["CreateTaskRequest"];
export type UpdateTaskRequest = S["UpdateTaskRequest"];
export type TaskSource = S["TaskSource"];
export type TaskState = S["TaskState"];
export type Repeat = S["Repeat"];
export type DismissedReason = S["DismissedReason"];
export type WeatherSummary = S["WeatherSummary"];
export type GardenWeather = S["GardenWeather"];
export type ForecastDay = S["ForecastDay"];
export type Memory = S["Memory"];
export type MemoryKind = S["MemoryKind"];
export type CreateMemoryRequest = S["CreateMemoryRequest"];
export type GardenMap = S["GardenMap"];
export type GardenMapInput = S["GardenMapInput"];
export type Bed = S["Bed"];
export type Pin = S["Pin"];

// Chat
export type AnchorType = S["AnchorType"];
export type Conversation = S["Conversation"];
export type ConversationDetail = S["ConversationDetail"];
export type ChatEvent = S["ChatEvent"];
export type PendingAction = S["PendingAction"];
export type SendMessageRequest = S["SendMessageRequest"];

// Connectors
export type Connector = S["Connector"];
export type ConnectorKind = S["ConnectorKind"];
export type ConnectorStatus = S["ConnectorStatus"];

/** `{ items, next_cursor? }` (contracts 1, lists). */
export type Page<T> = { items: T[]; next_cursor?: Cursor };
