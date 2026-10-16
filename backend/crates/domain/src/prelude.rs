//! The items most crates use: `use domain::prelude::*;`.

pub use crate::{
    authz::{
        AccessError, AtLeast, ConversationOwner, CurrentUser, Editor, Membership, MinRole, Owner,
        Reviewer, Viewer,
    },
    error::{DomainError, ErrorBody, ErrorCode, ErrorKind, InvalidStateDetails},
    events::{ProblemKind, ScanEvent, ScanEventBody, Stage},
    id::{
        ChatImageId, ConversationId, DiagnosisId, GardenId, Id, IdempotencyKey, ImageId,
        IndicatorId, JobId, LeaseToken, MaskId, MemoryId, MessageId, ObservationId, OrganismId,
        PendingActionId, PlanId, PlantId, ProblemId, QuestionId, RegionId, ScanId, Slug, SourceId,
        SpeciesId, Table, TaskId, TreatmentId, UserId, WorkerId,
    },
    jobs::{Capability, Job, JobKind, JobSpec},
    ports::{
        BlobError, BlobStore, ChatModel, Clock, IdGen, IdGenExt, KnowledgeStore, MemoryStore,
        ModelError, ProviderError, Store, StoreError, TextEmbedder, UrlSigner, VisionModel,
    },
    scan::{ImageKind, Scan, ScanDetail, ScanStatus},
    url::{Sign, Signed, Signer, Stored, UrlState},
    wire::{
        BBox, Certainty, Confidence, ConfidenceBand, EventSeq, LocalDate, Page, PageRequest, Rated,
        Region, Role, Run, Timestamp, V1,
    },
};
