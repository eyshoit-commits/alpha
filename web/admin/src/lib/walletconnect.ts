/**
 * WalletConnect SDK Integration for bKG Wallet
 * Enables dApp connections using WalletConnect 2.0 protocol
 */

import { Core } from '@walletconnect/core';
import { Web3Wallet, IWeb3Wallet } from '@walletconnect/web3wallet';
import { getSdkError } from '@walletconnect/utils';

export interface WalletConnectConfig {
  projectId: string;
  metadata: {
    name: string;
    description: string;
    url: string;
    icons: string[];
  };
}

export interface SessionProposal {
  id: number;
  params: {
    proposer: {
      metadata: {
        name: string;
        description: string;
        url: string;
        icons: string[];
      };
    };
    requiredNamespaces: Record<string, any>;
  };
}

export class WalletConnectService {
  private web3wallet: IWeb3Wallet | null = null;
  private initialized = false;

  constructor(private config: WalletConnectConfig) {}

  /**
   * Initialize WalletConnect Web3Wallet
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;

    const core = new Core({
      projectId: this.config.projectId,
    });

    this.web3wallet = await Web3Wallet.init({
      core,
      metadata: this.config.metadata,
    });

    this.setupEventListeners();
    this.initialized = true;
  }

  /**
   * Setup event listeners for WalletConnect
   */
  private setupEventListeners(): void {
    if (!this.web3wallet) return;

    // Session proposal
    this.web3wallet.on('session_proposal', async (proposal) => {
      console.log('Session proposal received:', proposal);
      // Emit event to UI for user approval
      window.dispatchEvent(new CustomEvent('wc:session_proposal', { detail: proposal }));
    });

    // Session request (transaction signing, etc.)
    this.web3wallet.on('session_request', async (request) => {
      console.log('Session request received:', request);
      window.dispatchEvent(new CustomEvent('wc:session_request', { detail: request }));
    });

    // Session delete
    this.web3wallet.on('session_delete', (session) => {
      console.log('Session deleted:', session);
      window.dispatchEvent(new CustomEvent('wc:session_delete', { detail: session }));
    });
  }

  /**
   * Pair with dApp using URI
   */
  async pair(uri: string): Promise<void> {
    if (!this.web3wallet) {
      throw new Error('WalletConnect not initialized');
    }
    await this.web3wallet.core.pairing.pair({ uri });
  }

  /**
   * Approve session proposal
   */
  async approveSession(
    proposalId: number,
    accounts: string[],
    chains: string[] = ['neo3:mainnet']
  ): Promise<void> {
    if (!this.web3wallet) {
      throw new Error('WalletConnect not initialized');
    }

    const proposal = this.web3wallet.getPendingSessionProposals()[proposalId];
    if (!proposal) {
      throw new Error('Proposal not found');
    }

    const namespaces = {
      neo3: {
        methods: [
          'invokeFunction',
          'testInvoke',
          'signMessage',
          'verifyMessage',
          'getWalletInfo',
        ],
        chains,
        events: ['chainChanged', 'accountChanged'],
        accounts: accounts.map((account) => `neo3:mainnet:${account}`),
      },
    };

    await this.web3wallet.approveSession({
      id: proposalId,
      namespaces,
    });
  }

  /**
   * Reject session proposal
   */
  async rejectSession(proposalId: number, reason?: string): Promise<void> {
    if (!this.web3wallet) {
      throw new Error('WalletConnect not initialized');
    }

    await this.web3wallet.rejectSession({
      id: proposalId,
      reason: getSdkError('USER_REJECTED'),
    });
  }

  /**
   * Disconnect session
   */
  async disconnectSession(topic: string): Promise<void> {
    if (!this.web3wallet) {
      throw new Error('WalletConnect not initialized');
    }

    await this.web3wallet.disconnectSession({
      topic,
      reason: getSdkError('USER_DISCONNECTED'),
    });
  }

  /**
   * Get active sessions
   */
  getActiveSessions(): Record<string, any> {
    if (!this.web3wallet) {
      return {};
    }
    return this.web3wallet.getActiveSessions();
  }

  /**
   * Get pending proposals
   */
  getPendingProposals(): Record<number, SessionProposal> {
    if (!this.web3wallet) {
      return {};
    }
    return this.web3wallet.getPendingSessionProposals();
  }

  /**
   * Respond to session request (sign transaction, etc.)
   */
  async respondToRequest(
    topic: string,
    id: number,
    response: { result?: any; error?: any }
  ): Promise<void> {
    if (!this.web3wallet) {
      throw new Error('WalletConnect not initialized');
    }

    if (response.error) {
      await this.web3wallet.respondSessionRequest({
        topic,
        response: {
          id,
          jsonrpc: '2.0',
          error: response.error,
        },
      });
    } else {
      await this.web3wallet.respondSessionRequest({
        topic,
        response: {
          id,
          jsonrpc: '2.0',
          result: response.result,
        },
      });
    }
  }
}

// Singleton instance
let walletConnectService: WalletConnectService | null = null;

export function getWalletConnectService(config?: WalletConnectConfig): WalletConnectService {
  if (!walletConnectService && config) {
    walletConnectService = new WalletConnectService(config);
  }
  if (!walletConnectService) {
    throw new Error('WalletConnect service not initialized');
  }
  return walletConnectService;
}

export function initializeWalletConnect(config: WalletConnectConfig): Promise<void> {
  const service = getWalletConnectService(config);
  return service.initialize();
}
