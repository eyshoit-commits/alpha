"use client";

import { useState, useEffect } from "react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { blockchain, BlockchainAccount } from "@/lib/blockchain";
import { Alert } from "@/components/ui/alert";
import { WalletConnectModal } from "@/components/wallet-connect-modal";

interface Transaction {
  id: string;
  from: string;
  to: string;
  amount: number;
  timestamp: string;
  status: "pending" | "confirmed" | "failed";
}

export default function WalletPage() {
  const [account, setAccount] = useState<BlockchainAccount | null>(null);
  const [balance, setBalance] = useState(1000); // Mock bKG balance
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [recipient, setRecipient] = useState("");
  const [amount, setAmount] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);

  useEffect(() => {
    // Load wallet from localStorage
    const savedWallet = localStorage.getItem("bkg-wallet");
    if (savedWallet) {
      setAccount(JSON.parse(savedWallet));
    }
    
    // Load transactions
    const savedTxs = localStorage.getItem("bkg-transactions");
    if (savedTxs) {
      setTransactions(JSON.parse(savedTxs));
    }
  }, []);

  const handleCreateWallet = () => {
    try {
      const newAccount = blockchain.createAccount();
      setAccount(newAccount);
      localStorage.setItem("bkg-wallet", JSON.stringify(newAccount));
      setSuccess("✅ Wallet created successfully!");
      setError(null);
    } catch (err) {
      setError("Failed to create wallet: " + String(err));
    }
  };

  const handleSendBKG = () => {
    try {
      if (!account) {
        setError("No wallet connected");
        return;
      }
      if (!recipient || !amount) {
        setError("Please enter recipient and amount");
        return;
      }

      const amountNum = parseFloat(amount);
      if (amountNum > balance) {
        setError("Insufficient balance");
        return;
      }

      // Create transaction
      const tx: Transaction = {
        id: `tx_${Date.now()}`,
        from: account.address,
        to: recipient,
        amount: amountNum,
        timestamp: new Date().toISOString(),
        status: "pending",
      };

      const newTransactions = [tx, ...transactions];
      setTransactions(newTransactions);
      setBalance(balance - amountNum);
      localStorage.setItem("bkg-transactions", JSON.stringify(newTransactions));

      // Simulate confirmation after 2 seconds
      setTimeout(() => {
        setTransactions((prev) =>
          prev.map((t) => (t.id === tx.id ? { ...t, status: "confirmed" as const } : t))
        );
      }, 2000);

      setSuccess(`✅ Sent ${amountNum} bKG to ${recipient.substring(0, 10)}...`);
      setRecipient("");
      setAmount("");
      setError(null);
    } catch (err) {
      setError("Transaction failed: " + String(err));
    }
  };

  const handleReceiveBKG = () => {
    if (!account) return;
    navigator.clipboard.writeText(account.address);
    setSuccess("📋 Address copied! Share it to receive bKG tokens");
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case "confirmed":
        return "text-[#5bec92]";
      case "pending":
        return "text-[#AF75FF]";
      case "failed":
        return "text-[#D3188C]";
      default:
        return "text-slate-400";
    }
  };

  const getStatusIcon = (status: string) => {
    switch (status) {
      case "confirmed":
        return "✅";
      case "pending":
        return "⏳";
      case "failed":
        return "❌";
      default:
        return "•";
    }
  };

  return (
    <section className="space-y-6">
      <Card
        title="💰 bKG Wallet"
        description="Manage your bKG tokens and blockchain transactions"
        actions={account && <WalletConnectModal account={account} />}
      >
        {error && <Alert variant="error" message={error} />}
        {success && <Alert variant="success" message={success} />}
      </Card>

      {!account ? (
        <Card title="Create Your Wallet" description="Get started with bKG tokens">
          <div className="text-center space-y-4">
            <p className="text-slate-300">You don't have a wallet yet. Create one to start using bKG tokens.</p>
            <Button onClick={handleCreateWallet} variant="primary" size="lg">
              🔑 Create New Wallet
            </Button>
          </div>
        </Card>
      ) : (
        <>
          <div className="grid gap-6 md:grid-cols-3">
            <Card title="💎 Balance" description="Your bKG tokens">
              <div className="text-center py-6">
                <div className="text-5xl font-black bg-gradient-to-r from-[#75ffaf] via-[#AF75FF] to-[#D3188C] bg-clip-text text-transparent">
                  {balance.toFixed(2)}
                </div>
                <div className="text-lg text-slate-400 mt-2">bKG</div>
              </div>
            </Card>

            <Card title="📍 Your Address" description="Share to receive tokens">
              <div className="space-y-3">
                <code className="block text-xs bg-[#0a0e27] p-3 rounded border border-[#5bec92]/40 text-[#5bec92] break-all">
                  {account.address}
                </code>
                <Button onClick={handleReceiveBKG} className="w-full">
                  📋 Copy Address
                </Button>
              </div>
            </Card>

            <Card title="📊 Statistics" description="Wallet activity">
              <div className="space-y-3 text-sm">
                <div className="flex justify-between">
                  <span className="text-slate-400">Transactions:</span>
                  <span className="text-[#75ffaf] font-bold">{transactions.length}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-400">Confirmed:</span>
                  <span className="text-[#5bec92] font-bold">
                    {transactions.filter((t) => t.status === "confirmed").length}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-400">Pending:</span>
                  <span className="text-[#AF75FF] font-bold">
                    {transactions.filter((t) => t.status === "pending").length}
                  </span>
                </div>
              </div>
            </Card>
          </div>

          <Card title="💸 Send bKG" description="Transfer tokens to another address">
            <div className="space-y-4">
              <Input
                label="Recipient Address"
                value={recipient}
                onChange={(e) => setRecipient(e.target.value)}
                placeholder="Enter NEO address..."
              />
              <Input
                label="Amount (bKG)"
                type="number"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                placeholder="0.00"
              />
              <div className="flex gap-3">
                <Button onClick={handleSendBKG} variant="primary" disabled={!recipient || !amount}>
                  🚀 Send bKG
                </Button>
                <Button onClick={() => setAmount(balance.toString())} variant="secondary">
                  Max
                </Button>
              </div>
            </div>
          </Card>

          <Card
            title="📜 Transaction History"
            description={`${transactions.length} transactions`}
          >
            {transactions.length === 0 ? (
              <p className="text-center text-slate-400 py-8">No transactions yet</p>
            ) : (
              <div className="space-y-3">
                {transactions.map((tx) => (
                  <div
                    key={tx.id}
                    className="rounded-lg bg-[#0a0e27] p-4 border border-[#5bec92]/30 hover:border-[#5bec92]/60 transition-all"
                  >
                    <div className="flex items-start justify-between">
                      <div className="flex-1">
                        <div className="flex items-center gap-2 mb-2">
                          <span className={`text-xl ${getStatusColor(tx.status)}`}>
                            {getStatusIcon(tx.status)}
                          </span>
                          <span className={`text-sm font-bold ${getStatusColor(tx.status)}`}>
                            {tx.status.toUpperCase()}
                          </span>
                        </div>
                        <div className="text-sm space-y-1">
                          <div className="flex gap-2">
                            <span className="text-slate-500">From:</span>
                            <code className="text-xs text-slate-300">
                              {tx.from.substring(0, 20)}...
                            </code>
                          </div>
                          <div className="flex gap-2">
                            <span className="text-slate-500">To:</span>
                            <code className="text-xs text-slate-300">
                              {tx.to.substring(0, 20)}...
                            </code>
                          </div>
                          <div className="text-xs text-slate-500">
                            {new Date(tx.timestamp).toLocaleString()}
                          </div>
                        </div>
                      </div>
                      <div className="text-right">
                        <div className="text-2xl font-bold text-[#D3188C]">
                          -{tx.amount.toFixed(2)}
                        </div>
                        <div className="text-sm text-slate-400">bKG</div>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </Card>
        </>
      )}
    </section>
  );
}
