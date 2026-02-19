/**
 * ACP Type Definitions
 * Agent Communication Protocol types for TypeScript
 */

// ============================================================================
// Agent Types
// ============================================================================

export type TransportType = 'pty' | 'stdio' | 'websocket' | 'http';

export interface Capability {
  id: string;
  name: string;
  tags?: string[];
}

export interface AgentCard {
  id: string;
  protocol: string;
  name: string;
  capabilities: Capability[];
  endpoint: string;
  transport: TransportType;
}

// ============================================================================
// Message Types
// ============================================================================

export type MessageType = 'prompt' | 'response' | 'broadcast' | 'discover' | 'advertise' | 'error';

export type Priority = 'low' | 'normal' | 'high';

export interface MessageMetadata {
  priority?: Priority;
  ttl?: number;
  correlationId?: string;
}

export interface MessagePayload {
  content: string;
  data?: Record<string, unknown>;
}

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
// Discovery Types
// ============================================================================

export interface DiscoveryQuery {
  capabilities?: string[];
  tags?: string[];
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
// Orchestrator Types
// ============================================================================

export interface OrchestratorStats {
  totalAgents: number;
  availableAgents: number;
  tasksCompleted: number;
  tasksFailed: number;
  tasksInProgress: number;
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
