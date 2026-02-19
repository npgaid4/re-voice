/**
 * Tauri IPC Transport for ACP
 */

import { invoke } from '@tauri-apps/api/core';
import type { AgentCard, TaskResult } from '../types';

/**
 * Tauri transport implementation
 */
export class TauriTransport {
  /**
   * Register an agent
   */
  async registerAgent(agentType: string, instanceId: string): Promise<string> {
    return await invoke<string>('acp_register_agent', { agentType, instanceId });
  }

  /**
   * Discover agents
   */
  async discoverAgents(query: {
    capabilities?: string[];
    tags?: string[];
    transport?: string;
  }): Promise<AgentCard[]> {
    return await invoke<AgentCard[]>('acp_discover_agents', query);
  }

  /**
   * List all agents
   */
  async listAgents(): Promise<AgentCard[]> {
    return await invoke<AgentCard[]>('acp_list_agents');
  }

  /**
   * Get agent by ID
   */
  async getAgent(agentId: string): Promise<AgentCard | null> {
    return await invoke<AgentCard | null>('acp_get_agent', { agentId });
  }

  /**
   * Send message
   */
  async sendMessage(to: string, content: string, from: string): Promise<string> {
    return await invoke<string>('acp_send_message', { to, content, from });
  }

  /**
   * Broadcast message
   */
  async broadcast(
    content: string,
    capabilities: string[] | null,
    from: string
  ): Promise<string[]> {
    return await invoke<string[]>('acp_broadcast', { content, capabilities, from });
  }

  /**
   * Get task state
   */
  async getTask(taskId: string): Promise<TaskResult | null> {
    return await invoke<TaskResult | null>('acp_get_task', { taskId });
  }

  /**
   * Get stats
   */
  async getStats(): Promise<{
    totalAgents: number;
    availableAgents: number;
    tasksCompleted: number;
    tasksFailed: number;
    tasksInProgress: number;
  }> {
    return await invoke('acp_stats');
  }
}

export const tauriTransport = new TauriTransport();
