"use client";

import { useState, useEffect } from "react";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { getWalletConnectService, initializeWalletConnect, SessionProposal } from "@/lib/walletconnect";

interface WalletConnectModalProps {
  account: { address: string } | null;
}

export function WalletConnectModal({ account }: WalletConnectModalProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [wcUri, setWcUri] = useState("");
  const [sessions, setSessions] = useState<Record<string, any>>({});
  const [proposals, setProposals] = useState<Record<number, SessionProposal>>({});
  const [isInitialized, setIsInitialized] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Initialize WalletConnect
    const init = async () => {
      try {
        await initializeWalletConnect({
          projectId: process.env.NEXT_PUBLIC_WC_PROJECT_ID || "demo-project-id",
          metadata: {
            name: "bKG Wallet",
            description: "bKG Platform Blockchain Wallet",
            url: "https://bkg.example",
            icons: ["https://bkg.example/icon.png"],
          },
        });
        setIsInitialized(true);
      } catch (err) {
        console.error("Failed to initialize WalletConnect:", err);
        setError("Failed to initialize WalletConnect");
      }
    };

    init();

    // Listen for WalletConnect events
    const handleProposal = (event: CustomEvent) => {
      const proposal = event.detail;
      setProposals((prev) => ({ ...prev, [proposal.id]: proposal }));
    };

    const handleSessionDelete = () => {
      updateSessions();
    };

    window.addEventListener('wc:session_proposal', handleProposal as EventListener);
    window.addEventListener('wc:session_delete', handleSessionDelete);

    return () => {
      window.removeEventListener('wc:session_proposal', handleProposal as EventListener);
      window.removeEventListener('wc:session_delete', handleSessionDelete);
    };
  }, []);

  const updateSessions = () => {
    try {
      const wc = getWalletConnectService();
      setSessions(wc.getActiveSessions());
      setProposals(wc.getPendingProposals());
    } catch (err) {
      console.error("Failed to update sessions:", err);
    }
  };

  const handleConnect = async () => {
    if (!wcUri) {
      setError("Please enter WalletConnect URI");
      return;
    }

    try {
      const wc = getWalletConnectService();
      await wc.pair(wcUri);
      setWcUri("");
      setError(null);
      updateSessions();
    } catch (err) {
      setError("Failed to connect: " + String(err));
    }
  };

  const handleApproveProposal = async (proposalId: number) => {
    if (!account) {
      setError("No wallet account available");
      return;
    }

    try {
      const wc = getWalletConnectService();
      await wc.approveSession(proposalId, [account.address]);
      setError(null);
      updateSessions();
    } catch (err) {
      setError("Failed to approve session: " + String(err));
    }
  };

  const handleRejectProposal = async (proposalId: number) => {
    try {
      const wc = getWalletConnectService();
      await wc.rejectSession(proposalId);
      setError(null);
      updateSessions();
    } catch (err) {
      setError("Failed to reject session: " + String(err));
    }
  };

  const handleDisconnect = async (topic: string) => {
    try {
      const wc = getWalletConnectService();
      await wc.disconnectSession(topic);
      setError(null);
      updateSessions();
    } catch (err) {
      setError("Failed to disconnect: " + String(err));
    }
  };

  if (!isOpen) {
    return (
      <Button onClick={() => setIsOpen(true)} variant="secondary">
        🔗 WalletConnect
      </Button>
    );
  }

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="max-w-2xl w-full max-h-[90vh] overflow-y-auto">
        <Card
          title="🔗 WalletConnect"
          description="Connect to dApps using WalletConnect 2.0"
        >
          <div className="space-y-6">
            {error && (
              <div className="bg-red-900/20 border border-red-500/50 text-red-300 p-3 rounded-lg">
                {error}
              </div>
            )}

            {!isInitialized ? (
              <div className="text-center py-8 text-slate-400">
                Initializing WalletConnect...
              </div>
            ) : (
              <>
                {/* Connect Section */}
                <div className="space-y-3">
                  <h3 className="text-lg font-bold text-[#75ffaf]">Connect to dApp</h3>
                  <Input
                    label="WalletConnect URI"
                    value={wcUri}
                    onChange={(e) => setWcUri(e.target.value)}
                    placeholder="wc:..."
                  />
                  <Button onClick={handleConnect} disabled={!wcUri}>
                    🔗 Connect
                  </Button>
                </div>

                {/* Pending Proposals */}
                {Object.keys(proposals).length > 0 && (
                  <div className="space-y-3">
                    <h3 className="text-lg font-bold text-[#AF75FF]">Pending Requests</h3>
                    {Object.entries(proposals).map(([id, proposal]) => (
                      <div
                        key={id}
                        className="rounded-lg bg-[#0a0e27] p-4 border border-[#AF75FF]/40"
                      >
                        <div className="flex items-start gap-3 mb-3">
                          {proposal.params.proposer.metadata.icons?.[0] && (
                            <img
                              src={proposal.params.proposer.metadata.icons[0]}
                              alt="dApp"
                              className="w-12 h-12 rounded-lg"
                            />
                          )}
                          <div>
                            <div className="font-bold text-[#AF75FF]">
                              {proposal.params.proposer.metadata.name}
                            </div>
                            <div className="text-sm text-slate-400">
                              {proposal.params.proposer.metadata.description}
                            </div>
                            <a
                              href={proposal.params.proposer.metadata.url}
                              target="_blank"
                              rel="noopener noreferrer"
                              className="text-xs text-[#5bec92] hover:underline"
                            >
                              {proposal.params.proposer.metadata.url}
                            </a>
                          </div>
                        </div>
                        <div className="flex gap-2">
                          <Button
                            onClick={() => handleApproveProposal(proposal.id)}
                            variant="primary"
                            size="sm"
                          >
                            ✓ Approve
                          </Button>
                          <Button
                            onClick={() => handleRejectProposal(proposal.id)}
                            variant="danger"
                            size="sm"
                          >
                            ✕ Reject
                          </Button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}

                {/* Active Sessions */}
                <div className="space-y-3">
                  <h3 className="text-lg font-bold text-[#5bec92]">
                    Active Connections ({Object.keys(sessions).length})
                  </h3>
                  {Object.keys(sessions).length === 0 ? (
                    <p className="text-slate-400 text-sm">No active connections</p>
                  ) : (
                    Object.entries(sessions).map(([topic, session]) => (
                      <div
                        key={topic}
                        className="rounded-lg bg-[#0a0e27] p-4 border border-[#5bec92]/40"
                      >
                        <div className="flex items-start justify-between">
                          <div className="flex items-start gap-3">
                            {session.peer?.metadata?.icons?.[0] && (
                              <img
                                src={session.peer.metadata.icons[0]}
                                alt="dApp"
                                className="w-10 h-10 rounded-lg"
                              />
                            )}
                            <div>
                              <div className="font-bold text-[#5bec92]">
                                {session.peer?.metadata?.name || "Unknown dApp"}
                              </div>
                              <div className="text-xs text-slate-500">
                                {session.peer?.metadata?.url}
                              </div>
                            </div>
                          </div>
                          <Button
                            onClick={() => handleDisconnect(topic)}
                            variant="danger"
                            size="sm"
                          >
                            Disconnect
                          </Button>
                        </div>
                      </div>
                    ))
                  )}
                </div>
              </>
            )}

            <Button onClick={() => setIsOpen(false)} variant="secondary" className="w-full">
              ✕ Close
            </Button>
          </div>
        </Card>
      </div>
    </div>
  );
}
