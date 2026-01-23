const hre = require("hardhat");
require("dotenv").config();

// Helper to wait between transactions
const sleep = (ms) => new Promise(resolve => setTimeout(resolve, ms));
const TX_DELAY = 3000; // 3 seconds between transactions

async function main() {
  const contractAddress = process.env.CONTRACT_ADDRESS;
  if (!contractAddress) {
    console.error("ERROR: CONTRACT_ADDRESS not set in .env file");
    process.exit(1);
  }

  console.log("=".repeat(60));
  console.log("Testing Proctoring Contract");
  console.log("=".repeat(60));
  console.log("Contract Address:", contractAddress);
  console.log("Network:", hre.network.name);
  console.log("");

  // Get signer
  const [signer] = await hre.ethers.getSigners();
  console.log("Signer Address:", signer.address);

  // Get balance
  const balance = await hre.ethers.provider.getBalance(signer.address);
  console.log("Signer Balance:", hre.ethers.formatEther(balance), "DEV");
  console.log("");

  // Get contract
  const contract = await hre.ethers.getContractAt("Proctoring", contractAddress);

  // Generate unique room ID for testing
  const roomId = `test-${Date.now()}`;
  const studentWallet = "0xa34BDC34aeA7b544dB23F004Cd3E0ea03aB217E0";

  console.log("=".repeat(60));
  console.log("Test 1: Create Room");
  console.log("=".repeat(60));
  console.log("Room ID:", roomId);
  console.log("Proctor:", signer.address);

  try {
    const tx1 = await contract.recordRoomCreated(roomId, signer.address, "Test Proctor");
    console.log("Transaction sent:", tx1.hash);
    const receipt1 = await tx1.wait();
    console.log("Transaction confirmed in block:", receipt1.blockNumber);
    console.log("Gas used:", receipt1.gasUsed.toString());
    console.log("Status:", receipt1.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Verify room was created
  console.log("Verifying room creation...");
  try {
    const roomInfo = await contract.getRoomInfo(roomId);
    console.log("Room Info:");
    console.log("  - Proctor:", roomInfo[0]);
    console.log("  - Proctor Name:", roomInfo[1]);
    console.log("  - Created At:", new Date(Number(roomInfo[2]) * 1000).toISOString());
    console.log("  - Participant Count:", roomInfo[4].toString());
    console.log("  - Status:", roomInfo[5] === 0n ? "Active" : "Closed");
  } catch (e) {
    console.log("ERROR reading room:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 2: Record Participant Joined (Student)");
  console.log("=".repeat(60));
  console.log("Student:", studentWallet);

  try {
    const tx2 = await contract.recordParticipantJoined(roomId, studentWallet, "Test Student", 1); // 1 = Student
    console.log("Transaction sent:", tx2.hash);
    const receipt2 = await tx2.wait();
    console.log("Transaction confirmed in block:", receipt2.blockNumber);
    console.log("Gas used:", receipt2.gasUsed.toString());
    console.log("Status:", receipt2.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 3: Record Recording Started");
  console.log("=".repeat(60));

  try {
    const tx3 = await contract.recordRecordingStarted(roomId, studentWallet);
    console.log("Transaction sent:", tx3.hash);
    const receipt3 = await tx3.wait();
    console.log("Transaction confirmed in block:", receipt3.blockNumber);
    console.log("Gas used:", receipt3.gasUsed.toString());
    console.log("Status:", receipt3.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 4: Create Exam Result");
  console.log("=".repeat(60));

  let examResultId;
  try {
    const tx4 = await contract.createExamResult(roomId, studentWallet, 8500, "Test Exam"); // 8500 = 85.00%
    console.log("Transaction sent:", tx4.hash);
    const receipt4 = await tx4.wait();
    console.log("Transaction confirmed in block:", receipt4.blockNumber);
    console.log("Gas used:", receipt4.gasUsed.toString());
    console.log("Status:", receipt4.status === 1 ? "SUCCESS" : "FAILED");

    // Get exam result ID from event
    const event = receipt4.logs.find(log => {
      try {
        const parsed = contract.interface.parseLog(log);
        return parsed?.name === "ExamResultCreated";
      } catch {
        return false;
      }
    });
    if (event) {
      const parsed = contract.interface.parseLog(event);
      examResultId = parsed.args[0];
      console.log("Exam Result ID:", examResultId.toString());
    }
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 5: Record Recording Stopped (with IPFS CID)");
  console.log("=".repeat(60));

  const testIpfsCid = "QmTestCid123456789abcdef";
  try {
    const tx5 = await contract.recordRecordingStopped(roomId, studentWallet, 300, testIpfsCid); // 300 seconds
    console.log("Transaction sent:", tx5.hash);
    const receipt5 = await tx5.wait();
    console.log("Transaction confirmed in block:", receipt5.blockNumber);
    console.log("Gas used:", receipt5.gasUsed.toString());
    console.log("Status:", receipt5.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 6: Record Participant Left");
  console.log("=".repeat(60));

  try {
    const tx6 = await contract.recordParticipantLeft(roomId, studentWallet, 0); // 0 = Normal
    console.log("Transaction sent:", tx6.hash);
    const receipt6 = await tx6.wait();
    console.log("Transaction confirmed in block:", receipt6.blockNumber);
    console.log("Gas used:", receipt6.gasUsed.toString());
    console.log("Status:", receipt6.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  // Wait before next transaction
  console.log(`Waiting ${TX_DELAY/1000}s before next transaction...`);
  await sleep(TX_DELAY);

  console.log("=".repeat(60));
  console.log("Test 7: Close Room");
  console.log("=".repeat(60));

  try {
    const tx7 = await contract.closeRoom(roomId, 0); // 0 = ProctorLeft
    console.log("Transaction sent:", tx7.hash);
    const receipt7 = await tx7.wait();
    console.log("Transaction confirmed in block:", receipt7.blockNumber);
    console.log("Gas used:", receipt7.gasUsed.toString());
    console.log("Status:", receipt7.status === 1 ? "SUCCESS" : "FAILED");
  } catch (e) {
    console.log("ERROR:", e.reason || e.message);
    return;
  }
  console.log("");

  console.log("=".repeat(60));
  console.log("Verification: Read Data from Contract");
  console.log("=".repeat(60));

  // Check participant exam results
  try {
    const examIds = await contract.getParticipantExamResultIds(studentWallet);
    console.log("Student exam result IDs:", examIds.map(id => id.toString()));

    if (examIds.length > 0) {
      const latestExamId = examIds[examIds.length - 1];
      const examResult = await contract.getExamResult(latestExamId);
      console.log("Latest Exam Result:");
      console.log("  - ID:", examResult[0].toString());
      console.log("  - Room ID:", examResult[1]);
      console.log("  - Participant:", examResult[2]);
      console.log("  - Grade:", (Number(examResult[3]) / 100).toFixed(2) + "%");
      console.log("  - Exam Name:", examResult[4]);
      console.log("  - Created At:", new Date(Number(examResult[5]) * 1000).toISOString());
      console.log("  - NFT Minted:", examResult[7]);
      console.log("  - Recording Count:", examResult[8].toString());

      const recordings = await contract.getExamResultRecordings(latestExamId);
      console.log("  - Recordings:", recordings);
    }
  } catch (e) {
    console.log("ERROR reading exam results:", e.reason || e.message);
  }

  // Check participant rooms
  try {
    const rooms = await contract.getParticipantRooms(studentWallet);
    console.log("Student rooms:", rooms);
  } catch (e) {
    console.log("ERROR reading rooms:", e.reason || e.message);
  }

  // Check total exam results
  try {
    const total = await contract.getTotalExamResults();
    console.log("Total exam results on contract:", total.toString());
  } catch (e) {
    console.log("ERROR reading total:", e.reason || e.message);
  }

  console.log("");
  console.log("=".repeat(60));
  console.log("All tests completed!");
  console.log("=".repeat(60));
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
