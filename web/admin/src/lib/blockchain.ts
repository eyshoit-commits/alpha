/**
 * NEO Blockchain Integration Plugin
 * Using neon-js for blockchain operations
 */

import Neon from "@cityofzion/neon-js";

export interface BlockchainAccount {
  address: string;
  publicKey: string;
  privateKey?: string;
}

export class BlockchainPlugin {
  private static instance: BlockchainPlugin;

  private constructor() {}

  static getInstance(): BlockchainPlugin {
    if (!BlockchainPlugin.instance) {
      BlockchainPlugin.instance = new BlockchainPlugin();
    }
    return BlockchainPlugin.instance;
  }

  /**
   * Create a new NEO blockchain account
   */
  createAccount(): BlockchainAccount {
    const account = Neon.create.account();
    return {
      address: account.address,
      publicKey: account.publicKey,
      privateKey: account.privateKey,
    };
  }

  /**
   * Import account from private key
   */
  importAccount(privateKey: string): BlockchainAccount {
    const account = Neon.create.account(privateKey);
    return {
      address: account.address,
      publicKey: account.publicKey,
      privateKey: account.privateKey,
    };
  }

  /**
   * Validate NEO address
   */
  isValidAddress(address: string): boolean {
    try {
      return Neon.is.address(address);
    } catch {
      return false;
    }
  }

  /**
   * Validate private key
   */
  isValidPrivateKey(privateKey: string): boolean {
    try {
      return Neon.is.privateKey(privateKey);
    } catch {
      return false;
    }
  }

  /**
   * Get balance for an address (requires RPC endpoint)
   */
  async getBalance(address: string, rpcUrl: string): Promise<any> {
    try {
      const rpcClient = new Neon.rpc.RPCClient(rpcUrl);
      const balance = await rpcClient.getBalance(address);
      return balance;
    } catch (error) {
      console.error("Error fetching balance:", error);
      throw error;
    }
  }

  /**
   * Sign message with private key
   */
  signMessage(message: string, privateKey: string): string {
    const account = Neon.create.account(privateKey);
    const signature = Neon.sign.message(message, account.privateKey);
    return signature;
  }

  /**
   * Verify signed message
   */
  verifySignature(message: string, signature: string, publicKey: string): boolean {
    try {
      return Neon.verify.message(message, signature, publicKey);
    } catch {
      return false;
    }
  }

  /**
   * Generate random private key
   */
  generatePrivateKey(): string {
    return Neon.create.privateKey();
  }

  /**
   * Encrypt private key with passphrase
   */
  encryptPrivateKey(privateKey: string, passphrase: string): string {
    const account = Neon.create.account(privateKey);
    return account.encrypt(passphrase);
  }

  /**
   * Decrypt private key with passphrase
   */
  decryptPrivateKey(encryptedKey: string, passphrase: string): string {
    const account = Neon.create.account(encryptedKey);
    return account.decrypt(passphrase).privateKey;
  }
}

// Export singleton instance
export const blockchain = BlockchainPlugin.getInstance();

// Export Neon for advanced usage
export { Neon };
