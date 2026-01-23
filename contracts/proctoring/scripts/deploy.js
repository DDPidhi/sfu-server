const { ethers, network } = require("hardhat");

async function main() {
  const networkName = network.name;
  const chainId = network.config.chainId;

  console.log(`Deploying Proctoring contract to ${networkName} (chainId: ${chainId})...\n`);

  // Get the deployer account
  const [deployer] = await ethers.getSigners();
  console.log("Deployer address:", deployer.address);

  // Check balance
  const balance = await ethers.provider.getBalance(deployer.address);
  const symbol = networkName === "moonbaseAlpha" ? "DEV" : "PAS";
  console.log("Deployer balance:", ethers.formatEther(balance), symbol + "\n");

  if (balance === 0n) {
    console.error("Error: Deployer account has no balance!");
    if (networkName === "moonbaseAlpha") {
      console.error("Get DEV tokens from: https://faucet.moonbeam.network/");
    } else {
      console.error("Get testnet tokens from the Paseo faucet.");
    }
    process.exit(1);
  }

  // Deploy the contract
  console.log("Deploying contract...");
  const Proctoring = await ethers.getContractFactory("Proctoring");
  const proctoring = await Proctoring.deploy();

  await proctoring.waitForDeployment();
  const contractAddress = await proctoring.getAddress();

  console.log("\n========================================");
  console.log("Proctoring contract deployed!");
  console.log("Contract address:", contractAddress);
  console.log("Owner:", deployer.address);
  console.log("========================================\n");

  console.log("Add this to your .env file:");
  console.log(`ASSET_HUB_CONTRACT_ADDRESS=${contractAddress}`);

  // Verify the deployment
  const owner = await proctoring.owner();
  console.log("\nVerification:");
  console.log("Contract owner:", owner);
  console.log("Deployment successful:", owner === deployer.address);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
