"use client";

import { useState, useEffect } from "react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface Block {
  number: number;
  hash: string;
  timestamp: string;
  transactions: number;
  validator: string;
  size: number;
}

interface ExplorerTransaction {
  hash: string;
  block: number;
  from: string;
  to: string;
  amount: number;
  fee: number;
  timestamp: string;
  status: "confirmed" | "pending";
}

export default function ExplorerPage() {
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [transactions, setTransactions] = useState<ExplorerTransaction[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedBlock, setSelectedBlock] = useState<Block | null>(null);
  const [stats, setStats] = useState({
    totalBlocks: 12547,
    totalTransactions: 89234,
    totalAddresses: 4521,
    avgBlockTime: 15,
    networkHashrate: "1.2 TH/s",
    difficulty: 8234567,
  });

  useEffect(() => {
    // Generate mock blockchain data
    const mockBlocks: Block[] = Array.from({ length: 10 }, (_, i) => ({
      number: 12547 - i,
      hash: `0x${Math.random().toString(16).substring(2, 66)}`,
      timestamp: new Date(Date.now() - i * 15000).toISOString(),
      transactions: Math.floor(Math.random() * 50) + 1,
      validator: `validator_${Math.floor(Math.random() * 10)}`,
      size: Math.floor(Math.random() * 100000) + 50000,
    }));
    setBlocks(mockBlocks);

    // Generate mock transactions
    const mockTxs: ExplorerTransaction[] = Array.from({ length: 20 }, (_, i) => ({
      hash: `0x${Math.random().toString(16).substring(2, 66)}`,
      block: 12547 - Math.floor(i / 2),
      from: `N${Math.random().toString(36).substring(2, 35)}`,
      to: `N${Math.random().toString(36).substring(2, 35)}`,
      amount: Math.random() * 1000,
      fee: Math.random() * 0.01,
      timestamp: new Date(Date.now() - i * 7500).toISOString(),
      status: i < 2 ? "pending" : "confirmed",
    }));
    setTransactions(mockTxs);
  }, []);

  const handleSearch = () => {
    if (!searchQuery) return;
    // TODO: Implement actual search
    alert(`Searching for: ${searchQuery}`);
  };

  const formatHash = (hash: string) => {
    return `${hash.substring(0, 10)}...${hash.substring(hash.length - 8)}`;
  };

  const formatAddress = (address: string) => {
    return `${address.substring(0, 8)}...${address.substring(address.length - 6)}`;
  };

  return (
    <section className="space-y-6">
      <Card
        title="🔍 bKG Blockchain Explorer"
        description="Explore blocks, transactions, and addresses on the bKG blockchain"
      />

      {/* Network Statistics */}
      <div className="grid gap-4 md:grid-cols-3 lg:grid-cols-6">
        <Card title="📦 Total Blocks" className="text-center">
          <div className="text-3xl font-black text-[#5bec92]">
            {stats.totalBlocks.toLocaleString()}
          </div>
        </Card>
        <Card title="💸 Total Transactions" className="text-center">
          <div className="text-3xl font-black text-[#AF75FF]">
            {stats.totalTransactions.toLocaleString()}
          </div>
        </Card>
        <Card title="👥 Addresses" className="text-center">
          <div className="text-3xl font-black text-[#75ffaf]">
            {stats.totalAddresses.toLocaleString()}
          </div>
        </Card>
        <Card title="⏱️ Block Time" className="text-center">
          <div className="text-3xl font-black text-[#D3188C]">
            {stats.avgBlockTime}s
          </div>
        </Card>
        <Card title="⚡ Hashrate" className="text-center">
          <div className="text-2xl font-black text-[#5bec92]">
            {stats.networkHashrate}
          </div>
        </Card>
        <Card title="🎯 Difficulty" className="text-center">
          <div className="text-2xl font-black text-[#AF75FF]">
            {(stats.difficulty / 1000000).toFixed(1)}M
          </div>
        </Card>
      </div>

      {/* Search */}
      <Card title="🔎 Search" description="Find blocks, transactions, or addresses">
        <div className="flex gap-3">
          <Input
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Enter block number, tx hash, or address..."
            onKeyPress={(e) => e.key === "Enter" && handleSearch()}
          />
          <Button onClick={handleSearch}>🔍 Search</Button>
        </div>
      </Card>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* Latest Blocks */}
        <Card
          title="📦 Latest Blocks"
          description={`Last ${blocks.length} blocks`}
        >
          <div className="space-y-2">
            {blocks.map((block) => (
              <div
                key={block.number}
                onClick={() => setSelectedBlock(block)}
                className="rounded-lg bg-[#0a0e27] p-4 border border-[#5bec92]/30 hover:border-[#5bec92]/70 hover:bg-[#1a1f3a] transition-all cursor-pointer"
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-3">
                    <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-[#5bec92] to-[#75ffaf] flex items-center justify-center text-lg font-black">
                      📦
                    </div>
                    <div>
                      <div className="font-bold text-[#5bec92]">Block #{block.number}</div>
                      <div className="text-xs text-slate-500">
                        {new Date(block.timestamp).toLocaleTimeString()}
                      </div>
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-sm font-bold text-[#AF75FF]">
                      {block.transactions} txs
                    </div>
                    <div className="text-xs text-slate-500">
                      {(block.size / 1024).toFixed(1)} KB
                    </div>
                  </div>
                </div>
                <div className="text-xs">
                  <code className="text-slate-400">{formatHash(block.hash)}</code>
                </div>
              </div>
            ))}
          </div>
        </Card>

        {/* Latest Transactions */}
        <Card
          title="💸 Latest Transactions"
          description={`Last ${transactions.length} transactions`}
        >
          <div className="space-y-2">
            {transactions.map((tx) => (
              <div
                key={tx.hash}
                className="rounded-lg bg-[#0a0e27] p-4 border border-[#AF75FF]/30 hover:border-[#AF75FF]/70 hover:bg-[#1a1f3a] transition-all"
              >
                <div className="flex items-center justify-between mb-2">
                  <div>
                    <code className="text-xs text-[#AF75FF]">{formatHash(tx.hash)}</code>
                    <div className="text-xs text-slate-500 mt-1">
                      Block #{tx.block} • {new Date(tx.timestamp).toLocaleTimeString()}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-lg font-bold text-[#D3188C]">
                      {tx.amount.toFixed(4)} bKG
                    </div>
                    <div className="text-xs text-slate-500">Fee: {tx.fee.toFixed(4)}</div>
                  </div>
                </div>
                <div className="text-xs space-y-1">
                  <div className="flex gap-2">
                    <span className="text-slate-500">From:</span>
                    <code className="text-slate-300">{formatAddress(tx.from)}</code>
                  </div>
                  <div className="flex gap-2">
                    <span className="text-slate-500">To:</span>
                    <code className="text-slate-300">{formatAddress(tx.to)}</code>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>

      {/* Block Details Modal */}
      {selectedBlock && (
        <Card
          title={`📦 Block #${selectedBlock.number} Details`}
          description="Detailed information about this block"
        >
          <div className="space-y-4">
            <div className="grid md:grid-cols-2 gap-4">
              <div>
                <div className="text-sm text-slate-400 mb-1">Block Hash</div>
                <code className="text-xs bg-[#0a0e27] p-2 rounded border border-[#5bec92]/40 block text-[#5bec92] break-all">
                  {selectedBlock.hash}
                </code>
              </div>
              <div>
                <div className="text-sm text-slate-400 mb-1">Validator</div>
                <code className="text-xs bg-[#0a0e27] p-2 rounded border border-[#AF75FF]/40 block text-[#AF75FF]">
                  {selectedBlock.validator}
                </code>
              </div>
              <div>
                <div className="text-sm text-slate-400 mb-1">Timestamp</div>
                <div className="text-base text-slate-300">
                  {new Date(selectedBlock.timestamp).toLocaleString()}
                </div>
              </div>
              <div>
                <div className="text-sm text-slate-400 mb-1">Transactions</div>
                <div className="text-base text-slate-300 font-bold">
                  {selectedBlock.transactions} transactions
                </div>
              </div>
            </div>
            <Button onClick={() => setSelectedBlock(null)} variant="secondary">
              ✕ Close
            </Button>
          </div>
        </Card>
      )}
    </section>
  );
}
