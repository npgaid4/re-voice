/**
 * ACP Client
 * Client for the Agent Communication Protocol
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  AgentCard,
  DiscoveryQuery,
  TaskState,
  OrchestratorStats,
  SharedContext,
} from './types';

export type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

/**
 * ACP Client class for communicating with agents via the ACP protocol
 */
export class ACPClient {
  private invokeFn: InvokeFn;

  constructor(invokeFn?: InvokeFn) {
    // Use provided invoke function or default to Tauri's invoke
    this.invokeFn = invokeFn || ((cmd: string, args?: Record<string, unknown>) => invoke(cmd, args));
  }

  // ==========================================================================
  // Agent Management
  // ==========================================================================

  /**
   * Register a new agent
   * @param agentType Type of agent (e.g., 'claude-code')
   * @param instanceId Unique instance identifier
   * @returns Agent ID
   */
  async registerAgent(agentType: string, instanceId: string): Promise<string> {
    const result = await this.invokeFn('acp_register_agent', {
      agentType,
      instanceId,
    });
    return result as string;
  }

  /**
   * Discover agents matching a query
   * @param query Discovery query
   * @returns Matching agents
   */
  async discoverAgents(query: DiscoveryQuery = {}): Promise<AgentCard[]> {
    const result = await this.invokeFn('acp_discover_agents', {
      capabilities: query.capabilities,
      tags: query.tags,
      transport: query.transport,
    });
    return result as AgentCard[];
  }

  /**
   * List all registered agents
   * @returns All agents
   */
  async listAgents(): Promise<AgentCard[]> {
    const result = await this.invokeFn('acp_list_agents');
    return result as AgentCard[];
  }

  /**
   * Get a specific agent by ID
   * @param agentId Agent ID
   * @returns Agent card or null
   */
  async getAgent(agentId: string): Promise<AgentCard | null> {
    const result = await this.invokeFn('acp_get_agent', { agentId });
    return result as AgentCard | null;
  }

  // ==========================================================================
  // Messaging
  // ==========================================================================

  /**
   * Send a message to a specific agent
   * @param to Target agent ID
   * @param content Message content
   * @param from Source agent ID (defaults to 'app')
   * @returns Response from agent
   */
  async send(to: string, content: string, from = 'app'): Promise<string> {
    const result = await this.invokeFn('acp_send_message', {
      to,
      content,
      from,
    });
    return result as string;
  }

  /**
   * Broadcast a message to agents with specific capabilities
   * @param content Message content
   * @param filter Filter options
   * @returns Responses from all matching agents
   */
  async broadcast(
    content: string,
    filter?: { capabilities?: string[] }
  ): Promise<string[]> {
    const result = await this.invokeFn('acp_broadcast', {
      content,
      capabilities: filter?.capabilities,
      from: 'app',
    });
    return result as string[];
  }

  // ==========================================================================
  // Task Management
  // ==========================================================================

  /**
   * Get task state by ID
   * @param taskId Task ID
   * @returns Task state or null
   */
  async getTask(taskId: string): Promise<TaskState | null> {
    const result = await this.invokeFn('acp_get_task', { taskId });
    return result as TaskState | null;
  }

  // ==========================================================================
  // Statistics
  // ==========================================================================

  /**
   * Get orchestrator statistics
   * @returns Current statistics
   */
  async getStats(): Promise<OrchestratorStats> {
    const result = await this.invokeFn('acp_stats');
    return result as OrchestratorStats;
  }

  // ==========================================================================
  // Context
  // ==========================================================================

  /**
   * Get shared context
   * @returns Current shared context
   */
  async getContext(): Promise<SharedContext> {
    const result = await this.invokeFn('acp_get_context');
    return result as SharedContext;
  }

  // ==========================================================================
  // Convenience Methods
  // ==========================================================================

  /**
   * Find agents with a specific capability
   * @param capability Capability ID
   * @returns Matching agents
   */
  async findByCapability(capability: string): Promise<AgentCard[]> {
    return this.discoverAgents({ capabilities: [capability] });
  }

  /**
   * Find translation-capable agents
   * @returns Translation agents
   */
  async findTranslators(): Promise<AgentCard[]> {
    return this.findByCapability('translation');
  }

  /**
   * Find code-generation capable agents
   * @returns Code generation agents
   */
  async findCodeGenerators(): Promise<AgentCard[]> {
    return this.findByCapability('code-generation');
  }

  /**
   * Send a translation request
   * @param text Text to translate
   * @param targetLang Target language
   * @returns Translated text
   */
  async translate(text: string, targetLang: string): Promise<string> {
    const translators = await this.findTranslators();

    if (translators.length === 0) {
      throw new Error('No translation agents available');
    }

    const prompt = `Translate the following text to ${targetLang}:\n\n${text}`;
    return this.send(translators[0].id, prompt);
  }

  /**
   * Request code review from available agents
   * @param code Code to review
   * @param language Programming language
   * @returns Review responses
   */
  async reviewCode(code: string, language: string): Promise<string[]> {
    const prompt = `Please review the following ${language} code:\n\n\`\`\`${language}\n${code}\n\`\`\``;
    return this.broadcast(prompt, { capabilities: ['code-review'] });
  }
}

// Export singleton instance
export const acpClient = new ACPClient();

// Export types
export * from './types';
