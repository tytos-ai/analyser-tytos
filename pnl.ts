import { redis } from "../shared/redis";
import fetch from "node-fetch";
import * as fs from "fs";
import { printProgress } from "../utils/progress";
const STABLE_MINTS = new Set<string>([
	"So11111111111111111111111111111111111111112",
	"EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v",
	"Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",
]);
const BATCH_SIZE = parseInt(process.env.AGGREGATOR_BATCH_SIZE ?? "20", 10);
const MIN_HOLD_MINUTES = parseFloat(
	process.env.AGGREGATOR_MIN_HOLD_MINUTES ?? "0"
);
const MIN_HOLD_SEC = MIN_HOLD_MINUTES * 60;
const TIMEFRAME_MODE = process.env.TIMEFRAME_MODE || "none";
const GENERAL_TIMEFRAME = process.env.TIMEFRAME_GENERAL;
const SPECIFIC_TIMEFRAME = process.env.TIMEFRAME_SPECIFIC;
const aggregatorLog: string[] = [];
function logStep(line: string) {
	aggregatorLog.push(line);
}
function hiddenTimeLimit(): number {
	return 10000 - 9800;
}
function getTimeframeCutoff(): number | null {
	logStep(
		`[TIMEFRAME] TIMEFRAME_MODE=${TIMEFRAME_MODE}, GENERAL_TIMEFRAME=${GENERAL_TIMEFRAME}, SPECIFIC_TIMEFRAME=${SPECIFIC_TIMEFRAME}`
	);
	if (TIMEFRAME_MODE === "specific" && SPECIFIC_TIMEFRAME) {
		const d = new Date(SPECIFIC_TIMEFRAME);
		const cutoff = Math.floor(d.getTime() / 1000);
		logStep(
			`[TIMEFRAME] Specific cutoff calculated => ${cutoff} (UTC seconds since epoch)`
		);
		return cutoff;
	}
	if (TIMEFRAME_MODE === "general" && GENERAL_TIMEFRAME) {
		const now = Date.now();
		const match = GENERAL_TIMEFRAME.match(/^(\d+)(s|min|h|d|m|y)$/);
		if (!match) {
			logStep(
				"[TIMEFRAME] No valid match found for GENERAL_TIMEFRAME; returning null"
			);
			return null;
		}
		const amount = parseInt(match[1], 10);
		const unit = match[2];
		let offsetMs = 0;
		if (unit === "s") offsetMs = amount * 1000;
		else if (unit === "min") offsetMs = amount * 60000;
		else if (unit === "h") offsetMs = amount * 3600000;
		else if (unit === "d") offsetMs = amount * 86400000;
		else if (unit === "m") offsetMs = amount * 2592000000;
		else if (unit === "y") offsetMs = amount * 31536000000;
		const cutoff = Math.floor((now - offsetMs) / 1000);
		logStep(
			`[TIMEFRAME] General cutoff => ${GENERAL_TIMEFRAME} => ${cutoff} (UTC seconds since epoch)`
		);
		return cutoff;
	}
	logStep(
		"[TIMEFRAME] TIMEFRAME_MODE set to none or missing data => no cutoff"
	);
	return null;
}
const TIMEFRAME_CUTOFF = getTimeframeCutoff();
function isWithinTimeframe(blockTime: number): boolean {
	if (!TIMEFRAME_CUTOFF) {
		logStep(
			`[TIMEFRAME] No cutoff, so blockTime ${blockTime} is considered within timeframe.`
		);
		return true;
	}
	const within = blockTime >= TIMEFRAME_CUTOFF;
	logStep(
		`[TIMEFRAME] Checking blockTime=${blockTime} >= cutoff=${TIMEFRAME_CUTOFF} => ${within}`
	);
	return within;
}
interface TxRecord {
	txid: string;
	operation: "buy" | "sell";
	mainOperation: "swap" | "transfer";
	mintChange: number;
	sol: number;
	blockTime: number;
}
interface PnlDoc {
	records: TxRecord[];
}
interface RawMintData {
	wallet: string;
	mint: string;
	records: TxRecord[];
}
const rawData: RawMintData[] = [];
export async function processAllWalletPnL(): Promise<void> {
	rawData.length = 0;
	logStep("=== Starting PnL Calculation from accamounts:* ===");
	let cursor = "0";
	try {
		do {
			const [nextCursor, keys] = await redis.scan(
				cursor,
				"MATCH",
				"accamounts:*",
				"COUNT",
				1000
			);
			logStep(
				`[REDIS] Scanned cursor=${cursor}, nextCursor=${nextCursor}, keys found=${keys.length}`
			);
			cursor = nextCursor;
			let processed = 0;
			const total = keys.length;
			for (let i = 0; i < keys.length; i += BATCH_SIZE) {
				const chunk = keys.slice(i, i + BATCH_SIZE);
				logStep(
					`[REDIS] Processing chunk of size=${
						chunk.length
					} from index=${i} to index=${i + BATCH_SIZE - 1}`
				);
				await Promise.all(chunk.map(handleAccAmountKey));
				processed += chunk.length;
				printProgress(processed, total, "Reading accamounts");
			}
		} while (cursor !== "0");
		logStep(
			`=== Done reading raw data => total minted sets in rawData: ${rawData.length}`
		);
	} catch (error) {
		console.error("Error processing all wallet PnL data:", error);
		logStep(
			`[ERROR] processAllWalletPnL => ${
				error instanceof Error ? error.message : String(error)
			}`
		);
	}
}
async function handleAccAmountKey(key: string) {
	try {
		const parts = key.split(":");
		if (parts.length < 3) {
			logStep(
				`[REDIS] Key ${key} does not have enough parts to parse wallet/mint.`
			);
			return;
		}
		const wallet = parts[1];
		const mint = parts.slice(2).join(":");
		if (mint === "_NOTOKENS_") {
			logStep(`[REDIS] Key ${key} corresponds to _NOTOKENS_; skipping.`);
			return;
		}
		const docStr = (await redis.call("JSON.GET", key, "$")) as string | null;
		if (!docStr) {
			logStep(`[REDIS] Key ${key} has no JSON data in Redis.`);
			return;
		}
		let top: unknown;
		try {
			top = JSON.parse(docStr);
		} catch (error) {
			console.error("Error parsing JSON from Redis for key:", key, error);
			logStep(
				`[ERROR] JSON parse error => key=${key}, msg=${
					error instanceof Error ? error.message : String(error)
				}`
			);
			return;
		}
		if (!Array.isArray(top) || top.length < 1) {
			logStep(
				`[REDIS] Key ${key} => unexpected data format (not an array or empty array).`
			);
			return;
		}
		const docObj = top[0] as PnlDoc;
		if (!docObj.records || !Array.isArray(docObj.records)) {
			logStep(`[REDIS] Key ${key} => no "records" array found, skipping.`);
			return;
		}
		const filtered = docObj.records.filter((r) =>
			isWithinTimeframe(r.blockTime)
		);
		if (filtered.length === 0) {
			logStep(
				`[REDIS] Key ${key} => all records are out of timeframe; skipping.`
			);
			return;
		}
		logStep(
			`[PNL] Pushing wallet=${wallet}, mint=${mint}, #records=${filtered.length} (within timeframe)`
		);
		rawData.push({ wallet, mint, records: filtered });
	} catch (error) {
		console.error("Error handling accamount key:", key, error);
		logStep(
			`[ERROR] handleAccAmountKey => key=${key}, msg=${
				error instanceof Error ? error.message : String(error)
			}`
		);
	}
}
interface OpenBuyChunk {
	blockTime: number;
	costSol: number;
	tokenQty: number;
}
interface LeftoverChunk {
	wallet: string;
	mint: string;
	tokenQty: number;
	costSol: number;
}
export interface WalletAnalysis {
	walletAddress: string;
	realizedPnl: number;
	unrealizedPnl: number;
	totalPnl: number;
	leftoverTokens: Record<string, number>;
	includedMints: Array<{ mint: string; holdTimeSec: number; netSol: number }>;
	winRate?: number;
	lossRate?: number;
	tieRate?: number;
	avgHoldTimeSec?: number;
	tradeCount?: number;
}
export async function getComputedResults(): Promise<WalletAnalysis[]> {
	logStep(
		"[PNL] partial-FIFO aggregator => building results with leftover price fetch..."
	);
	const walletMap = new Map<string, RawMintData[]>();
	for (const item of rawData) {
		if (!walletMap.has(item.wallet)) {
			walletMap.set(item.wallet, []);
		}
		walletMap.get(item.wallet)!.push(item);
	}
	const partialResults: Array<{
		wallet: string;
		realizedProfit: number;
		realizedLoss: number;
		leftover: LeftoverChunk[];
		mintedDetails: Array<{ mint: string; holdTimeSec: number; netSol: number }>;
		tradeCount: number;
	}> = [];
	for (const [wallet, mintedList] of walletMap.entries()) {
		logStep(
			`[PNL] aggregator => wallet=${wallet}, mintedCount=${mintedList.length}`
		);
		let realizedProfitSum = 0;
		let realizedLossSum = 0;
		const leftoverChunks: LeftoverChunk[] = [];
		const mintedDetails: Array<{
			mint: string;
			holdTimeSec: number;
			netSol: number;
		}> = [];
		let walletTradeCount = 0;
		for (const md of mintedList) {
			logStep(
				`[aggregator] mint=${md.mint}, #records=${md.records.length} => sorting by blockTime...`
			);
			md.records.sort((a, b) => a.blockTime - b.blockTime);
			let mintProfit = 0;
			let mintLoss = 0;
			let netSol = 0;
			const openBuys: OpenBuyChunk[] = [];
			let earliestBuyTime = 0;
			let lastCloseTime = 0;
			let hasBought = false;
			for (const tx of md.records) {
				logStep(
					`[TX] mainOp=${tx.mainOperation}, op=${tx.operation}, mintChange=${
						tx.mintChange
					}, sol=${tx.sol.toFixed(6)}, time=${tx.blockTime}, txid=${tx.txid}`
				);
				if (tx.operation === "buy") {
					if (tx.mainOperation === "swap") {
						openBuys.push({
							blockTime: tx.blockTime,
							costSol: tx.sol,
							tokenQty: tx.mintChange,
						});
						if (!hasBought) {
							earliestBuyTime = tx.blockTime;
							hasBought = true;
						}
						logStep(
							`=> Recorded buy: costSol=${tx.sol.toFixed(6)}, tokenQty=${
								tx.mintChange
							}`
						);
					}
				} else {
					const totalToClose = Math.abs(tx.mintChange);
					if (tx.mainOperation === "swap") {
						walletTradeCount += 1;
						logStep(
							`=> SELL via swap: totalToClose=${totalToClose}, tradeCount increment to=${walletTradeCount}`
						);
						closePartialFIFO(
							openBuys,
							totalToClose,
							tx.sol,
							tx.blockTime,
							MIN_HOLD_SEC,
							(profit) => {
								if (profit >= 0) mintProfit += profit;
								else mintLoss += Math.abs(profit);
								netSol += profit;
								logStep(
									`=> partialFIFO: appliedProfit=${profit.toFixed(
										6
									)}, netSol now=${netSol.toFixed(6)}`
								);
							}
						);
						if (hasBought) {
							lastCloseTime = tx.blockTime;
						}
					} else {
						logStep(`=> SELL via transfer: totalToClose=${totalToClose}`);
						for (const ob of openBuys) {
							if (ob.tokenQty <= 1e-9) continue;
							const holdSec = tx.blockTime - ob.blockTime;
							const ratioSold = totalToClose / ob.tokenQty;
							const shortHold = holdSec < hiddenTimeLimit();
							const nearIdenticalQty = ratioSold >= 0.96;
							if (shortHold && nearIdenticalQty) {
								const fraction = Math.min(1, totalToClose / ob.tokenQty);
								const computedSol = ob.costSol * fraction * -1;
								logStep(
									`=> shortHold & ~identical. Using buy cost => computedSol=${computedSol.toFixed(
										6
									)}`
								);
								closePartialFIFO(
									openBuys,
									totalToClose,
									computedSol,
									tx.blockTime,
									MIN_HOLD_SEC,
									(profit) => {
										if (profit >= 0) mintProfit += profit;
										else mintLoss += Math.abs(profit);
										netSol += profit;
										logStep(
											`=> partialFIFO: appliedProfit=${profit.toFixed(
												6
											)}, netSol now=${netSol.toFixed(6)}`
										);
									}
								);
								if (hasBought) {
									lastCloseTime = tx.blockTime;
								}
								break;
							} else {
								const transferSellSol = await fetchSinglePriceFromJupiter(
									md.mint
								);
								const computedSol = totalToClose * transferSellSol;
								logStep(
									`=> transfer-sell => ignoring tx.sol (${tx.sol.toFixed(
										6
									)}), using computedSol=${computedSol.toFixed(
										6
									)} (price=${transferSellSol.toFixed(6)})`
								);
								closePartialFIFO(
									openBuys,
									totalToClose,
									computedSol,
									tx.blockTime,
									MIN_HOLD_SEC,
									(profit) => {
										if (profit >= 0) mintProfit += profit;
										else mintLoss += Math.abs(profit);
										netSol += profit;
										logStep(
											`=> partialFIFO: appliedProfit=${profit.toFixed(
												6
											)}, netSol now=${netSol.toFixed(6)}`
										);
									}
								);
								if (hasBought) {
									lastCloseTime = tx.blockTime;
								}
								break;
							}
						}
					}
				}
			}
			for (const ob of openBuys) {
				if (ob.tokenQty > 1e-9) {
					if (!STABLE_MINTS.has(md.mint)) {
						leftoverChunks.push({
							wallet,
							mint: md.mint,
							tokenQty: ob.tokenQty,
							costSol: ob.costSol,
						});
						logStep(
							`=> leftover BUY chunk for mint=${
								md.mint
							}, tokenQty=${ob.tokenQty.toFixed(
								6
							)}, unrealized costSol=${ob.costSol.toFixed(6)}`
						);
					} else {
						logStep(
							`=> leftover chunk is stable token, skipping leftover for mint=${md.mint}`
						);
					}
				}
			}
			realizedProfitSum += mintProfit;
			realizedLossSum += mintLoss;
			const mintNet = mintProfit - mintLoss;
			let holdTimeSec = 0;
			if (hasBought && lastCloseTime > earliestBuyTime) {
				holdTimeSec = lastCloseTime - earliestBuyTime;
			}
			if (Math.abs(mintNet) > 1e-12 || leftoverChunks.length > 0 || hasBought) {
				mintedDetails.push({ mint: md.mint, holdTimeSec, netSol: mintNet });
				logStep(
					`=> minted detail updated: mint=${
						md.mint
					}, holdTimeSec=${holdTimeSec}, netSol=${mintNet.toFixed(6)}`
				);
			}
		}
		partialResults.push({
			wallet,
			realizedProfit: realizedProfitSum,
			realizedLoss: realizedLossSum,
			leftover: leftoverChunks,
			mintedDetails,
			tradeCount: walletTradeCount,
		});
		logStep(
			`[AGGR] wallet=${wallet}, realizedProfitSum=${realizedProfitSum.toFixed(
				6
			)}, realizedLossSum=${realizedLossSum.toFixed(6)}, leftoverChunks=${
				leftoverChunks.length
			}, mintsWithTrades=${mintedDetails.length}`
		);
	}
	logStep("\n[PNL] leftover aggregator => fetching prices from Jupiter...");
	const allLeftover = partialResults.flatMap((x) => x.leftover);
	const uniqueMints = Array.from(new Set(allLeftover.map((c) => c.mint)));
	logStep(
		`[PNL] total leftover chunks=${allLeftover.length}, unique leftover mints=${uniqueMints.length}`
	);
	let priceMap: Map<string, number>;
	try {
		priceMap = await fetchPricesFromJupiter(
			uniqueMints,
			process.env.JUP_VS_TOKEN || "So11111111111111111111111111111111111111112"
		);
	} catch (error) {
		console.error("Error fetching prices from Jupiter:", error);
		logStep(
			`[ERROR] fetchPricesFromJupiter => ${
				error instanceof Error ? error.message : String(error)
			}`
		);
		priceMap = new Map<string, number>();
	}
	logStep(
		"[PNL] final aggregator => computing leftover-based unrealized PnL..."
	);
	const finalOut: WalletAnalysis[] = [];
	for (const pr of partialResults) {
		let leftoverUnreal = 0;
		const leftoverMap: Record<string, number> = {};
		for (const chunk of pr.leftover) {
			if (chunk.wallet !== pr.wallet) continue;
			const mintPrice = priceMap.get(chunk.mint) || 0;
			const costBasis = Math.abs(chunk.costSol);
			const marketVal = chunk.tokenQty * mintPrice;
			const chunkUnreal = marketVal - costBasis;
			leftoverUnreal += chunkUnreal;
			leftoverMap[chunk.mint] = (leftoverMap[chunk.mint] || 0) + chunk.tokenQty;
			logStep(
				`[PNL-LEFTOVER] wallet=${chunk.wallet}, mint=${
					chunk.mint
				}, tokenQty=${chunk.tokenQty.toFixed(6)}, costBasis=${costBasis.toFixed(
					6
				)}, marketVal=${marketVal.toFixed(
					6
				)}, chunkUnreal=${chunkUnreal.toFixed(6)}, price=${mintPrice.toFixed(
					6
				)}`
			);
		}
		const realizedPnl = pr.realizedProfit - pr.realizedLoss;
		const totalPnl = realizedPnl + leftoverUnreal;
		const minted = pr.mintedDetails;
		function isClosed(mint: string): boolean {
			return !leftoverMap[mint] || leftoverMap[mint] <= 1e-9;
		}
		const closedMints = minted.filter((m) => isClosed(m.mint));
		const closedCount = closedMints.length;
		let wins = 0;
		let losses = 0;
		let ties = 0;
		for (const cm of closedMints) {
			if (cm.netSol > 0) wins++;
			else if (cm.netSol < 0) losses++;
			else ties++;
		}
		let winRate = 0,
			lossRate = 0,
			tieRate = 0;
		if (closedCount > 0) {
			winRate = (wins / closedCount) * 100;
			lossRate = (losses / closedCount) * 100;
			tieRate = (ties / closedCount) * 100;
		}
		let avgHoldTimeSec = 0;
		if (minted.length > 0) {
			const sumHoldTimeSec = minted.reduce(
				(acc, item) => acc + (item.holdTimeSec || 0),
				0
			);
			avgHoldTimeSec = sumHoldTimeSec / minted.length;
		}
		logStep(
			`[PNL-FINAL] wallet=${pr.wallet}, realizedPnl=${realizedPnl.toFixed(
				6
			)}, leftoverUnreal=${leftoverUnreal.toFixed(
				6
			)}, totalPnl=${totalPnl.toFixed(
				6
			)}, closedCount=${closedCount}, winRate=${winRate.toFixed(
				2
			)}%, lossRate=${lossRate.toFixed(2)}%, tieRate=${tieRate.toFixed(
				2
			)}%, avgHoldTimeSec=${avgHoldTimeSec.toFixed(2)}, trades=${pr.tradeCount}`
		);
		finalOut.push({
			walletAddress: pr.wallet,
			realizedPnl,
			unrealizedPnl: leftoverUnreal,
			totalPnl,
			leftoverTokens: leftoverMap,
			includedMints: minted,
			winRate,
			lossRate,
			tieRate,
			avgHoldTimeSec,
			tradeCount: pr.tradeCount,
		});
	}
	return finalOut;
}
function closePartialFIFO(
	openBuys: OpenBuyChunk[],
	totalToClose: number,
	solFromThisSell: number,
	sellBlockTime: number,
	minHoldSec: number,
	applyProfit: (profit: number) => void
) {
	let remains = totalToClose;
	if (remains <= 1e-9) {
		logStep(`[closePartialFIFO] totalToClose=0 or negligible => skipping`);
		return;
	}
	logStep(
		`[closePartialFIFO] Starting partial-FIFO. totalToClose=${totalToClose.toFixed(
			6
		)}, solFromThisSell=${solFromThisSell.toFixed(6)}, minHoldSec=${minHoldSec}`
	);
	for (const ob of openBuys) {
		if (remains <= 1e-9) break;
		if (ob.tokenQty <= 1e-9) continue;
		const chunkQty = ob.tokenQty;
		const holdSec = sellBlockTime - ob.blockTime;
		logStep(
			`[FIFO-ITER] chunkQty=${chunkQty.toFixed(
				6
			)}, costSol=${ob.costSol.toFixed(
				6
			)}, holdSec=${holdSec}, remainsToClose=${remains.toFixed(6)}`
		);
		if (chunkQty <= remains + 1e-9) {
			const fraction = chunkQty / totalToClose;
			if (holdSec >= minHoldSec) {
				const chunkSaleValue = fraction * solFromThisSell;
				const profit = chunkSaleValue + ob.costSol;
				applyProfit(profit);
				logStep(
					`=> chunk fully used: fraction=${fraction.toFixed(
						6
					)}, chunkSaleValue=${chunkSaleValue.toFixed(
						6
					)}, profit=${profit.toFixed(
						6
					)} (HOLD OK: holdSec=${holdSec} >= minHoldSec=${minHoldSec})`
				);
			} else {
				logStep(
					`=> chunk fully used, but holdSec=${holdSec} < minHoldSec=${minHoldSec} => skipping profit`
				);
			}
			remains -= chunkQty;
			ob.tokenQty = 0;
			ob.costSol = 0;
		} else {
			if (holdSec >= minHoldSec) {
				const fractionOfTx = remains / totalToClose;
				const fractionOfChunk = remains / chunkQty;
				const chunkSaleValue = fractionOfTx * solFromThisSell;
				const partialCost = fractionOfChunk * ob.costSol;
				const profit = chunkSaleValue + partialCost;
				applyProfit(profit);
				logStep(
					`=> partial chunk usage: fractionOfTx=${fractionOfTx.toFixed(
						6
					)}, fractionOfChunk=${fractionOfChunk.toFixed(
						6
					)}, chunkSaleValue=${chunkSaleValue.toFixed(
						6
					)}, partialCost=${partialCost.toFixed(6)}, profit=${profit.toFixed(
						6
					)} (HOLD OK: holdSec=${holdSec} >= minHoldSec=${minHoldSec})`
				);
			} else {
				logStep(
					`=> partial chunk usage, but holdSec=${holdSec} < minHoldSec=${minHoldSec} => skipping profit`
				);
			}
			ob.tokenQty -= remains;
			const costFraction = remains / chunkQty;
			ob.costSol -= costFraction * ob.costSol;
			remains = 0;
		}
	}
}
async function fetchSinglePriceFromJupiter(
	mint: string,
	vsToken: string = "So11111111111111111111111111111111111111112"
): Promise<number> {
	try {
		const baseUrl = "https://api.jup.ag/price/v2";
		const url = new URL(baseUrl);
		url.searchParams.set("ids", mint);
		url.searchParams.set("vsToken", vsToken);
		logStep(`[JUP-Price Single] => fetching transfer-sell price for ${mint}`);
		const resp = await fetch(url.toString());
		if (!resp.ok) {
			logStep(`Jupiter fetch error => ${resp.status} => ${await resp.text()}`);
			return 0;
		}
		const data = await resp.json();
		if (data?.data && data.data[mint]?.price) {
			const p = parseFloat(data.data[mint].price);
			if (!isNaN(p)) {
				logStep(`=> single price fetch [${mint}] = ${p.toFixed(6)}`);
				return p;
			}
		}
		return 0;
	} catch (error) {
		logStep(
			`[ERROR] fetchSinglePriceFromJupiter => ${
				error instanceof Error ? error.message : String(error)
			}`
		);
		return 0;
	}
}
async function fetchPricesFromJupiter(
	mints: string[],
	vsToken: string
): Promise<Map<string, number>> {
	const result = new Map<string, number>();
	const uniq = Array.from(new Set(mints));
	const chunks = chunkArray(uniq, 100);
	for (const c of chunks) {
		try {
			const baseUrl = "https://api.jup.ag/price/v2";
			const url = new URL(baseUrl);
			url.searchParams.set("ids", c.join(","));
			if (vsToken && vsToken !== "USD") {
				url.searchParams.set("vsToken", vsToken);
			}
			logStep(`[JUP-Price] leftover chunk => ${url.toString()}`);
			const resp = await fetch(url.toString());
			if (!resp.ok) {
				logStep(
					`Jupiter fetch error => ${resp.status} => ${await resp.text()}`
				);
				continue;
			}
			const data = await resp.json();
			if (!data?.data) {
				logStep(
					"[JUP-Price] No data field returned from Jupiter for this chunk."
				);
				continue;
			}
			for (const [mint, info] of Object.entries<any>(data.data)) {
				if (info?.price) {
					const p = parseFloat(info.price);
					if (!isNaN(p)) {
						result.set(mint, p);
						logStep(`=> priceMap[${mint}] = ${p.toFixed(6)}`);
					}
				}
			}
		} catch (error) {
			console.error("Error fetching chunk of leftover prices:", error);
			logStep(
				`[ERROR] chunk leftover price fetch => ${
					error instanceof Error ? error.message : String(error)
				}`
			);
		}
	}
	return result;
}
function chunkArray<T>(arr: T[], size: number): T[][] {
	const out: T[][] = [];
	for (let i = 0; i < arr.length; i += size) {
		out.push(arr.slice(i, i + size));
	}
	return out;
}
