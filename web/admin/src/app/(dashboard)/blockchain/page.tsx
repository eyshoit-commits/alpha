"use client";

import { useState } from "react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { blockchain, BlockchainAccount } from "@/lib/blockchain";
import { Alert } from "@/components/ui/alert";

export default function BlockchainPage() {
  const [account, setAccount] = useState<BlockchainAccount | null>(null);
  const [privateKey, setPrivateKey] = useState("");
  const [address, setAddress] = useState("");
  const [message, setMessage] = useState("");
  const [signature, setSignature] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  const handleCreateAccount = () => {
    try {
      const newAccount = blockchain.createAccount();
      setAccount(newAccount);
      setSuccess("✅ New blockchain account created!");
      setError(null);
    } catch (err) {
      setError("Failed to create account: " + String(err));
    }
  };

  const handleImportAccount = () => {
    try {
      if (!privateKey) {
        setError("Please enter a private key");
        return;
      }
      const importedAccount = blockchain.importAccount(privateKey);
      setAccount(importedAccount);
      setSuccess("✅ Account imported successfully!");
      setError(null);
    } catch (err) {
      setError("Failed to import account: " + String(err));
    }
  };

  const handleValidateAddress = () => {
    try {
      if (!address) {
        setError("Please enter an address");
        return;
      }
      const isValid = blockchain.isValidAddress(address);
      if (isValid) {
        setSuccess("✅ Valid NEO address!");
      } else {
        setError("❌ Invalid NEO address");
      }
    } catch (err) {
      setError("Validation error: " + String(err));
    }
  };

  const handleSignMessage = () => {
    try {
      if (!account?.privateKey) {
        setError("No account loaded");
        return;
      }
      if (!message) {
        setError("Please enter a message");
        return;
      }
      const sig = blockchain.signMessage(message, account.privateKey);
      setSignature(sig);
      setSuccess("✅ Message signed!");
      setError(null);
    } catch (err) {
      setError("Failed to sign message: " + String(err));
    }
  };

  const handleCopyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setSuccess("📋 Copied to clipboard!");
  };

  return (
    <section className="space-y-6">
      <Card
        title="🔗 NEO Blockchain Integration"
        description="Manage blockchain accounts, sign messages, and interact with the NEO blockchain"
      >
        {error && <Alert variant="error" message={error} />}
        {success && <Alert variant="success" message={success} />}
      </Card>

      <div className="grid gap-6 md:grid-cols-2">
        <Card title="Create New Account" description="Generate a new NEO blockchain account">
          <div className="space-y-4">
            <Button onClick={handleCreateAccount} variant="primary">
              🔑 Create New Account
            </Button>

            {account && (
              <div className="space-y-3 rounded-lg bg-[#0a0e27] p-4 border border-[#5bec92]/40">
                <div>
                  <p className="text-sm font-bold text-[#75ffaf] mb-1">Address:</p>
                  <div className="flex gap-2">
                    <code className="flex-1 text-xs bg-[#1a1f3a] p-2 rounded border border-[#5bec92]/30 text-slate-300 break-all">
                      {account.address}
                    </code>
                    <Button size="sm" onClick={() => handleCopyToClipboard(account.address)}>
                      📋
                    </Button>
                  </div>
                </div>
                <div>
                  <p className="text-sm font-bold text-[#75ffaf] mb-1">Public Key:</p>
                  <code className="block text-xs bg-[#1a1f3a] p-2 rounded border border-[#5bec92]/30 text-slate-300 break-all">
                    {account.publicKey}
                  </code>
                </div>
                <div>
                  <p className="text-sm font-bold text-[#D3188C] mb-1">Private Key (Keep Secret!):</p>
                  <div className="flex gap-2">
                    <code className="flex-1 text-xs bg-[#1a1f3a] p-2 rounded border border-[#D3188C]/30 text-[#D3188C] break-all">
                      {account.privateKey}
                    </code>
                    <Button size="sm" variant="danger" onClick={() => handleCopyToClipboard(account.privateKey || "")}>
                      📋
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </div>
        </Card>

        <Card title="Import Account" description="Import existing account from private key">
          <div className="space-y-4">
            <Input
              label="Private Key"
              value={privateKey}
              onChange={(e) => setPrivateKey(e.target.value)}
              placeholder="Enter NEO private key..."
              type="password"
            />
            <Button onClick={handleImportAccount} variant="secondary">
              📥 Import Account
            </Button>
          </div>
        </Card>

        <Card title="Validate Address" description="Check if a NEO address is valid">
          <div className="space-y-4">
            <Input
              label="NEO Address"
              value={address}
              onChange={(e) => setAddress(e.target.value)}
              placeholder="NKuyBkoGdZZSLyPbJEetheRhMjeznFZszf"
            />
            <Button onClick={handleValidateAddress}>
              ✓ Validate Address
            </Button>
          </div>
        </Card>

        <Card title="Sign Message" description="Cryptographically sign a message">
          <div className="space-y-4">
            <Input
              label="Message"
              value={message}
              onChange={(e) => setMessage(e.target.value)}
              placeholder="Enter message to sign..."
            />
            <Button onClick={handleSignMessage} disabled={!account}>
              ✍️ Sign Message
            </Button>

            {signature && (
              <div className="rounded-lg bg-[#0a0e27] p-4 border border-[#5bec92]/40">
                <p className="text-sm font-bold text-[#75ffaf] mb-2">Signature:</p>
                <div className="flex gap-2">
                  <code className="flex-1 text-xs bg-[#1a1f3a] p-2 rounded border border-[#5bec92]/30 text-slate-300 break-all">
                    {signature}
                  </code>
                  <Button size="sm" onClick={() => handleCopyToClipboard(signature)}>
                    📋
                  </Button>
                </div>
              </div>
            )}
          </div>
        </Card>
      </div>
    </section>
  );
}
