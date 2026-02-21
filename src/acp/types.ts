/**
 * A2A Protocol Type Definitions
 * Agent-to-Agent Communication Protocol
 * Based on: https://github.com/google/A2A
 */

// ============================================================================
// Protocol Version
// ============================================================================

export const A2A_PROTOCOL_VERSION = '0.3.0' as const;

// ============================================================================
// Transport Types (Internal)
// ============================================================================

export type TransportType = 'pty' | 'stdio' | 'websocket' | 'http';

// ============================================================================
// JSON Schema (for skill definitions)
// ============================================================================

export interface JSONSchema {
  type?: string;
  description?: string;
  properties?: Record<string, unknown>;
  required?: string[];
  items?: JSONSchema;
}

// ============================================================================
// Authentication
// ============================================================================

export interface Authentication {
  schemes: string[];
}

export const AuthSchemes = {
  apiKey: (): Authentication => ({ schemes: ['apiKey'] }),
  oauth2: (): Authentication => ({ schemes: ['OAuth2'] }),
  none: (): Authentication => ({ schemes: ['none'] }),
} as const;

// ============================================================================
// Provider (Organization Info)
// ============================================================================

export interface Provider {
  organization: string;
  url?: string;
}

// ============================================================================
// Agent Capabilities (A2A)
// ============================================================================

export interface AgentCapabilities {
  streaming: boolean;
  pushNotifications: boolean;
  stateTransitionHistory: boolean;
}

export const Capabilities = {
  default: (): AgentCapabilities => ({
    streaming: false,
    pushNotifications: false,
    stateTransitionHistory: false,
  }),
  withStreaming: (streaming: boolean = true): AgentCapabilities => ({
    streaming,
    pushNotifications: false,
    stateTransitionHistory: false,
  }),
} as const;

// ============================================================================
// Skill (A2A)
// ============================================================================

export interface Skill {
  id: string;
  name: string;
  description?: string;
  tags?: string[];
  examples?: string[];
  inputSchema?: JSONSchema;
  outputSchema?: JSONSchema;
  inputModes?: string[];
  outputModes?: string[];
}

// Legacy alias
export type Capability = Skill;

export function createSkill(
  id: string,
  name: string,
  options?: {
    description?: string;
    tags?: string[];
    examples?: string[];
    inputSchema?: JSONSchema;
    outputSchema?: JSONSchema;
    inputModes?: string[];
    outputModes?: string[];
  }
): Skill {
  return {
    id,
    name,
    ...options,
  };
}

// ============================================================================
// Agent Card (A2A Compliant)
// ============================================================================

/**
 * Agent Card - A2A compliant agent identity document
 * Should be hosted at: https://<base-url>/.well-known/agent.json
 */
export interface AgentCard {
  // A2A required fields
  name: string;
  url: string;
  version: string;
  protocolVersion: typeof A2A_PROTOCOL_VERSION;

  // A2A optional fields
  description?: string;
  provider?: Provider;
  capabilities?: AgentCapabilities;
  authentication?: Authentication;
  defaultInputModes?: string[];
  defaultOutputModes?: string[];
  skills?: Skill[];

  // Internal fields (not part of A2A spec)
  id?: string;
  transport?: TransportType;
}

export function createAgentCard(
  name: string,
  url: string,
  options?: {
    description?: string;
    version?: string;
    provider?: Provider;
    capabilities?: AgentCapabilities;
    authentication?: Authentication;
    defaultInputModes?: string[];
    defaultOutputModes?: string[];
    skills?: Skill[];
    id?: string;
    transport?: TransportType;
  }
): AgentCard {
  return {
    name,
    url,
    version: options?.version ?? '1.0.0',
    protocolVersion: A2A_PROTOCOL_VERSION,
    ...options,
  };
}

// Pre-built agent cards

export function claudeCodeAgentCard(instanceId: string): AgentCard {
  const id = `claude-code@localhost/${instanceId}`;
  const url = `acp://${id}`;

  return createAgentCard(
    `Claude Code (${instanceId})`,
    url,
    {
      id,
      transport: 'pty',
      description: "Anthropic's Claude Code CLI agent",
      version: '1.0.0',
      provider: {
        organization: 'Anthropic',
        url: 'https://anthropic.com',
      },
      capabilities: {
        streaming: true,
        pushNotifications: false,
        stateTransitionHistory: true,
      },
      authentication: { schemes: ['none'] },
      defaultInputModes: ['text/plain'],
      defaultOutputModes: ['text/plain', 'application/json'],
      skills: [
        createSkill('translation', 'Translation', {
          description: 'Translate text between languages',
          tags: ['multilingual'],
          examples: ['Translate this text to Japanese'],
          inputModes: ['text/plain'],
          outputModes: ['text/plain'],
        }),
        createSkill('code-generation', 'Code Generation', {
          description: 'Generate code in various programming languages',
          tags: ['programming', 'coding'],
          examples: ['Write a Python function to sort a list'],
          inputModes: ['text/plain'],
          outputModes: ['text/plain', 'application/json'],
        }),
        createSkill('code-review', 'Code Review', {
          description: 'Review code for quality and best practices',
          tags: ['programming'],
          examples: ['Review this pull request for potential issues'],
        }),
        createSkill('analysis', 'Analysis', {
          description: 'Analyze code, data, or text',
          tags: ['analysis'],
          examples: ['Analyze the architecture of this codebase'],
        }),
        createSkill('writing', 'Writing', {
          description: 'Generate written content',
          tags: ['content'],
          examples: ['Write documentation for this API'],
        }),
        createSkill('summarization', 'Summarization', {
          description: 'Summarize long texts or documents',
          tags: ['content'],
          examples: ['Summarize this research paper'],
        }),
      ],
    }
  );
}

export function codexAgentCard(instanceId: string): AgentCard {
  const id = `codex@localhost/${instanceId}`;
  const url = `acp://${id}`;

  return createAgentCard(
    `Codex (${instanceId})`,
    url,
    {
      id,
      transport: 'pty',
      description: 'OpenAI Codex agent',
      provider: {
        organization: 'OpenAI',
        url: 'https://openai.com',
      },
      capabilities: {
        streaming: true,
        pushNotifications: false,
        stateTransitionHistory: false,
      },
      skills: [
        createSkill('code-generation', 'Code Generation', {
          tags: ['programming'],
        }),
        createSkill('code-review', 'Code Review', {
          tags: ['programming'],
        }),
        createSkill('debugging', 'Debugging', {
          tags: ['programming'],
          examples: ['Find the bug in this code'],
        }),
      ],
    }
  );
}

// ============================================================================
// Address Types (Internal - for routing)
// ============================================================================

export interface AgentAddress {
  id: string;
  instance?: string;
}

export interface CapabilityFilter {
  capabilities?: string[];  // AND condition (skill IDs)
  tags?: string[];          // OR condition
  agentType?: string;
}

export interface PipelineStage {
  name: string;
  agent: AgentAddress;
  promptTemplate?: string;
}

export type AddressType =
  | { type: 'single'; address: AgentAddress }
  | { type: 'multiple'; addresses: AgentAddress[] }
  | { type: 'broadcast'; filter?: CapabilityFilter }
  | { type: 'pipeline'; stages: PipelineStage[] };

// ============================================================================
// Message Types
// ============================================================================

export type MessageType =
  // Basic
  | 'prompt' | 'response' | 'stream' | 'error'
  // Agent management
  | 'discover' | 'advertise' | 'heartbeat'
  // Control
  | 'cancel' | 'question' | 'answer'
  // Pipeline
  | 'pipeline_start' | 'pipeline_stage' | 'pipeline_end';

export type Priority = 'low' | 'normal' | 'high' | 'urgent';

// ============================================================================
// Message Metadata
// ============================================================================

export interface MessageMetadata {
  priority?: Priority;
  ttl?: number;
  traceId?: string;
  correlationId?: string;
}

export interface EnvelopeMetadata {
  priority?: Priority;
  ttl?: number;
  traceId?: string;
  correlationId?: string;
}

export interface MessagePayload {
  content: string;
  data?: Record<string, unknown>;
}

// ============================================================================
// Message Types
// ============================================================================

export interface ACPMessageV3 {
  id: string;
  timestamp: string;
  from: AgentAddress;
  to: AddressType;
  type: MessageType;
  payload: MessagePayload;
  metadata?: MessageMetadata;
}

export interface ACPEnvelope {
  protocol: string;
  message: ACPMessageV3;
  metadata?: EnvelopeMetadata;
}

// Legacy type
export interface ACPMessage {
  id: string;
  timestamp: string;
  from: string;
  to: string | string[];
  type: MessageType;
  payload: MessagePayload;
  metadata?: MessageMetadata;
}

// ============================================================================
// Discovery Query (A2A Compatible)
// ============================================================================

export interface DiscoveryQuery {
  capabilities?: string[];  // Skill IDs (AND condition)
  tags?: string[];          // Tags (OR condition)
  agentType?: string;
  streaming?: boolean;
  pushNotifications?: boolean;
  transport?: TransportType;
}

// ============================================================================
// Task Types
// ============================================================================

export type TaskExecutionStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface TaskState {
  taskId: string;
  messageId: string;
  from: string;
  to: string;
  status: TaskExecutionStatus;
  result?: TaskResult;
  error?: string;
}

export interface TaskResult {
  output: string;
  metadata?: Record<string, unknown>;
}

// ============================================================================
// Pipeline Types
// ============================================================================

export interface PipelineDefinition {
  id: string;
  name: string;
  stages: PipelineStage[];
  input?: Record<string, unknown>;
}

export interface PipelineExecution {
  pipelineId: string;
  executionId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  currentStage?: number;
  results: PipelineStageResult[];
  startTime: string;
  endTime?: string;
}

export interface PipelineStageResult {
  stageName: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'skipped';
  output?: unknown;
  error?: string;
  startTime: string;
  endTime?: string;
}

// ============================================================================
// Orchestrator Types
// ============================================================================

export interface OrchestratorStats {
  totalAgents: number;
  availableAgents: number;
  tasksCompleted: number;
  tasksFailed: number;
  tasksInProgress: number;
  pipelinesRunning: number;
}

// ============================================================================
// Context Types
// ============================================================================

export interface ContextEntry {
  agentId: string;
  summary: string;
  timestamp: string;
}

export interface SharedContext {
  conversationHistory: ContextEntry[];
  sharedFiles: string[];
  metadata: Record<string, unknown>;
}

// ============================================================================
// Event Types
// ============================================================================

export interface ACPEvent<T = unknown> {
  event: string;
  payload: T;
  timestamp: string;
}

export interface StatusChangedPayload {
  agentId: string;
  oldStatus: string;
  newStatus: string;
}

export interface OutputReadyPayload {
  agentId: string;
  content: string;
  messageId?: string;
}

export interface QuestionPayload {
  agentId: string;
  question: string;
  options?: string[];
  messageId: string;
}

export interface PipelineProgressPayload {
  pipelineId: string;
  executionId: string;
  stageIndex: number;
  stageName: string;
  status: string;
  progress?: number;
}

// ============================================================================
// Helper Functions
// ============================================================================

export function createAgentAddress(id: string, instance?: string): AgentAddress {
  return { id, instance };
}

export function parseAgentAddress(s: string): AgentAddress {
  const parts = s.split('/');
  if (parts.length === 2) {
    return { id: parts[0], instance: parts[1] };
  }
  return { id: s };
}

export function agentAddressToString(addr: AgentAddress): string {
  if (addr.instance) {
    return `${addr.id}/${addr.instance}`;
  }
  return addr.id;
}

export function createSingleAddress(address: string | AgentAddress): AddressType {
  const addr = typeof address === 'string' ? parseAgentAddress(address) : address;
  return { type: 'single', address: addr };
}

export function createMultipleAddresses(addresses: (string | AgentAddress)[]): AddressType {
  return {
    type: 'multiple',
    addresses: addresses.map(a => typeof a === 'string' ? parseAgentAddress(a) : a)
  };
}

export function createBroadcastAddress(filter?: CapabilityFilter): AddressType {
  return { type: 'broadcast', filter };
}

export function createPipelineAddress(stages: PipelineStage[]): AddressType {
  return { type: 'pipeline', stages };
}

// ============================================================================
// Agent State Types (CLI Executor v3)
// ============================================================================

/**
 * Claude Code エージェントの状態
 */
export type AgentState =
  | { type: 'initializing' }
  | { type: 'idle' }
  | { type: 'processing'; currentTool: string | null; startedAt: string }
  | { type: 'waiting_for_permission'; toolName: string; toolInput: unknown; requestId: string }
  | { type: 'waiting_for_input'; question: string; options: string[] }
  | { type: 'error'; message: string; recoverable: boolean }
  | { type: 'completed'; output: string };

/**
 * 状態遷移イベント
 */
export type StateEvent =
  | { event: 'initialized' }
  | { event: 'task_started'; prompt: string }
  | { event: 'tool_use_started'; toolName: string }
  | { event: 'tool_use_completed'; toolName: string; success: boolean }
  | { event: 'permission_required'; toolName: string; toolInput: unknown; requestId: string }
  | { event: 'permission_granted'; requestId: string }
  | { event: 'permission_denied'; requestId: string; reason: string }
  | { event: 'input_required'; question: string; options: string[] }
  | { event: 'input_received'; answer: string }
  | { event: 'error_occurred'; message: string; recoverable: boolean }
  | { event: 'task_completed'; output: string };

/**
 * 権限決定
 */
export type PermissionDecision =
  | { type: 'allow'; always: boolean }
  | { type: 'deny'; reason: string }
  | { type: 'require_human'; requestId: string; toolName: string; toolInput: unknown; options: string[] };

/**
 * 権限ポリシー
 */
export type PermissionPolicy = 'readOnly' | 'standard' | 'strict' | 'permissive';

/**
 * 権限要求
 */
export interface PermissionRequest {
  requestId: string;
  toolName: string;
  toolInput: unknown;
  options: string[];
  timestamp: string;
}

/**
 * エグゼキューターイベント
 */
export type ExecutorEvent =
  | { type: 'state_changed'; oldState: AgentState; newState: AgentState }
  | { type: 'output'; content: string }
  | { type: 'tool_execution'; name: string; input: unknown; result: string | null; isError: boolean }
  | { type: 'permission_required'; requestId: string; toolName: string; options: string[] }
  | { type: 'progress'; message: string; percentage: number }
  | { type: 'completed'; output: string }
  | { type: 'error'; message: string; recoverable: boolean };

/**
 * エグゼキューターオプション
 */
export interface ExecutorOptions {
  workingDir?: string;
  allowedTools?: string[];
  timeoutSecs?: number;
  sessionId?: string;
}

// ============================================================================
// Helper Functions for AgentState
// ============================================================================

/**
 * 状態名を取得
 */
export function getStateName(state: AgentState): string {
  return state.type;
}

/**
 * 処理中かどうか
 */
export function isProcessing(state: AgentState): boolean {
  return state.type === 'processing';
}

/**
 * アイドルまたは完了かどうか（タスク受付可能）
 */
export function isReady(state: AgentState): boolean {
  return state.type === 'idle' || state.type === 'completed';
}

/**
 * 待機状態かどうか（権限待ちまたは入力待ち）
 */
export function isWaiting(state: AgentState): boolean {
  return state.type === 'waiting_for_permission' || state.type === 'waiting_for_input';
}

/**
 * エラー状態かどうか
 */
export function isError(state: AgentState): boolean {
  return state.type === 'error';
}

/**
 * 完了状態かどうか
 */
export function isCompleted(state: AgentState): boolean {
  return state.type === 'completed';
}
