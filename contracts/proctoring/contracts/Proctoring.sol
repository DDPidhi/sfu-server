// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/**
 * @title Proctoring
 * @notice Smart contract for recording proctoring events on-chain with wallet-based identity
 * @dev Deployed on Asset Hub (Polkadot) via EVM
 *      Participants (students and proctors) are identified by their wallet addresses
 *      for future NFT generation based on exam results
 */
contract Proctoring {
    address public owner;

    // Enums
    enum RoomStatus { Active, Closed }
    enum Role { Proctor, Student }
    enum LeaveReason { Normal, Kicked, Disconnected, RoomClosed }
    enum VerificationStatus { Valid, Invalid, Pending, Skipped }
    enum SuspiciousActivityType { MultipleDevices, TabSwitch, WindowBlur, ScreenShare, UnauthorizedPerson, AudioAnomaly, Other }
    enum RoomCloseReason { ProctorLeft, SessionCompleted, AdminClosed, Timeout }

    // Structs
    struct RoomInfo {
        address proctor;
        string proctorName;
        uint256 createdAt;
        uint256 closedAt;
        uint32 participantCount;
        RoomStatus status;
        bool exists;
    }

    struct Participant {
        address wallet;
        string name;
        Role role;
        uint256 joinedAt;
        uint256 leftAt;
        bool exists;
    }

    struct ProctorEvent {
        uint8 eventType; // 0=RoomCreated, 1=ParticipantJoined, 2=ParticipantLeft, etc.
        address participant;
        string data; // JSON encoded additional data
        uint256 timestamp;
    }

    struct ExamResult {
        uint256 id;              // Unique result ID
        string roomId;           // Room where exam was taken
        address participant;     // Wallet address of the participant (student)
        uint256 grade;           // Score out of 10000 (allows 2 decimal precision, e.g., 8750 = 87.50%)
        string examName;         // Name of the exam
        uint256 createdAt;       // When the result was recorded
        uint256 updatedAt;       // Last update timestamp
        bool nftMinted;          // Whether NFT has been minted for this result
        bool exists;             // Whether this result exists
    }

    // Storage
    mapping(string => RoomInfo) public rooms;
    mapping(string => ProctorEvent[]) public roomEvents;

    // Participant tracking per room: roomId => participant wallet => Participant
    mapping(string => mapping(address => Participant)) public roomParticipants;
    // List of participant addresses per room
    mapping(string => address[]) public roomParticipantList;

    // Global participant data: wallet => name (cached across rooms)
    mapping(address => string) public participantNames;
    // Rooms a participant has joined: wallet => roomId[]
    mapping(address => string[]) public participantRooms;

    // Exam results with auto-incrementing ID
    uint256 public nextExamResultId;
    // Exam result by ID
    mapping(uint256 => ExamResult) public examResults;
    // Recording CIDs for each exam result: resultId => cid[]
    mapping(uint256 => string[]) public examResultRecordings;

    // Participant's exam result IDs: wallet => resultId[]
    mapping(address => uint256[]) public participantExamResults;
    // Quick lookup: roomId => participant wallet => result ID (0 means no result)
    mapping(string => mapping(address => uint256)) public roomParticipantResultId;

    // Events
    event RoomCreated(string indexed roomId, address indexed proctor, uint256 timestamp);
    event ParticipantJoined(string indexed roomId, address indexed participant, Role role, uint256 timestamp);
    event ParticipantLeft(string indexed roomId, address indexed participant, LeaveReason reason, uint256 timestamp);
    event ParticipantKicked(string indexed roomId, address indexed kicked, address indexed proctor, uint256 timestamp);
    event IdVerification(string indexed roomId, address indexed participant, VerificationStatus status, uint256 timestamp);
    event SuspiciousActivity(string indexed roomId, address indexed participant, SuspiciousActivityType activityType, uint256 timestamp);
    event RecordingStarted(string indexed roomId, address indexed participant, uint256 timestamp);
    event RecordingStopped(string indexed roomId, address indexed participant, uint64 durationSecs, string ipfsCid, uint256 timestamp);
    event RoomClosed(string indexed roomId, RoomCloseReason reason, uint256 timestamp);
    event ExamResultCreated(uint256 indexed resultId, string indexed roomId, address indexed participant, uint256 grade, uint256 timestamp);
    event RecordingAdded(uint256 indexed resultId, string ipfsCid, uint256 timestamp);
    event NftMinted(uint256 indexed resultId, address indexed participant, string indexed roomId, uint256 timestamp);

    // Modifiers
    modifier onlyOwner() {
        require(msg.sender == owner, "Only owner");
        _;
    }

    modifier roomExists(string memory roomId) {
        require(rooms[roomId].exists, "Room not found");
        _;
    }

    modifier roomActive(string memory roomId) {
        require(rooms[roomId].exists, "Room not found");
        require(rooms[roomId].status == RoomStatus.Active, "Room is closed");
        _;
    }

    modifier examResultExists(uint256 resultId) {
        require(examResults[resultId].exists, "Exam result not found");
        _;
    }

    constructor() {
        owner = msg.sender;
        nextExamResultId = 1; // Start from 1 so 0 can mean "no result"
    }

    /**
     * @notice Records a room creation event
     * @param roomId Unique room identifier
     * @param proctor Wallet address of the proctor creating the room
     * @param proctorName Display name of the proctor
     */
    function recordRoomCreated(
        string calldata roomId,
        address proctor,
        string calldata proctorName
    ) external {
        require(!rooms[roomId].exists, "Room already exists");
        require(proctor != address(0), "Invalid proctor address");

        rooms[roomId] = RoomInfo({
            proctor: proctor,
            proctorName: proctorName,
            createdAt: block.timestamp,
            closedAt: 0,
            participantCount: 1,
            status: RoomStatus.Active,
            exists: true
        });

        // Store proctor as first participant
        roomParticipants[roomId][proctor] = Participant({
            wallet: proctor,
            name: proctorName,
            role: Role.Proctor,
            joinedAt: block.timestamp,
            leftAt: 0,
            exists: true
        });
        roomParticipantList[roomId].push(proctor);

        // Cache proctor name globally
        if (bytes(participantNames[proctor]).length == 0) {
            participantNames[proctor] = proctorName;
        }
        participantRooms[proctor].push(roomId);

        roomEvents[roomId].push(ProctorEvent({
            eventType: 0,
            participant: proctor,
            data: proctorName,
            timestamp: block.timestamp
        }));

        emit RoomCreated(roomId, proctor, block.timestamp);
    }

    /**
     * @notice Records a participant joining
     * @param roomId Room identifier
     * @param participant Wallet address of the joining participant
     * @param name Display name of the participant
     * @param role Role of the participant (Proctor or Student)
     */
    function recordParticipantJoined(
        string calldata roomId,
        address participant,
        string calldata name,
        Role role
    ) external roomActive(roomId) {
        require(participant != address(0), "Invalid participant address");
        require(!roomParticipants[roomId][participant].exists, "Participant already in room");

        rooms[roomId].participantCount++;

        roomParticipants[roomId][participant] = Participant({
            wallet: participant,
            name: name,
            role: role,
            joinedAt: block.timestamp,
            leftAt: 0,
            exists: true
        });
        roomParticipantList[roomId].push(participant);

        // Cache participant name globally
        if (bytes(participantNames[participant]).length == 0 && bytes(name).length > 0) {
            participantNames[participant] = name;
        }
        participantRooms[participant].push(roomId);

        roomEvents[roomId].push(ProctorEvent({
            eventType: 1,
            participant: participant,
            data: name,
            timestamp: block.timestamp
        }));

        emit ParticipantJoined(roomId, participant, role, block.timestamp);
    }

    /**
     * @notice Records a participant leaving
     * @param roomId Room identifier
     * @param participant Wallet address of the leaving participant
     * @param reason Reason for leaving
     */
    function recordParticipantLeft(
        string calldata roomId,
        address participant,
        LeaveReason reason
    ) external roomExists(roomId) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");

        roomParticipants[roomId][participant].leftAt = block.timestamp;

        roomEvents[roomId].push(ProctorEvent({
            eventType: 2,
            participant: participant,
            data: "",
            timestamp: block.timestamp
        }));

        emit ParticipantLeft(roomId, participant, reason, block.timestamp);
    }

    /**
     * @notice Records a participant being kicked
     * @param roomId Room identifier
     * @param proctor Wallet address of the proctor performing the kick
     * @param kicked Wallet address of the kicked participant
     * @param reason Reason for kicking
     */
    function recordParticipantKicked(
        string calldata roomId,
        address proctor,
        address kicked,
        string calldata reason
    ) external roomActive(roomId) {
        require(rooms[roomId].proctor == proctor, "Only room proctor can kick");
        require(roomParticipants[roomId][kicked].exists, "Kicked participant not in room");

        roomParticipants[roomId][kicked].leftAt = block.timestamp;

        roomEvents[roomId].push(ProctorEvent({
            eventType: 3,
            participant: kicked,
            data: reason,
            timestamp: block.timestamp
        }));

        emit ParticipantKicked(roomId, kicked, proctor, block.timestamp);
    }

    /**
     * @notice Records an ID verification result
     * @param roomId Room identifier
     * @param participant Wallet address of the verified participant
     * @param status Verification status
     * @param verifiedBy Name/ID of the verifier
     */
    function recordIdVerification(
        string calldata roomId,
        address participant,
        VerificationStatus status,
        string calldata verifiedBy
    ) external roomActive(roomId) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");

        roomEvents[roomId].push(ProctorEvent({
            eventType: 4,
            participant: participant,
            data: verifiedBy,
            timestamp: block.timestamp
        }));

        emit IdVerification(roomId, participant, status, block.timestamp);
    }

    /**
     * @notice Records suspicious activity
     * @param roomId Room identifier
     * @param participant Wallet address of the participant with suspicious activity
     * @param activityType Type of suspicious activity
     * @param details Additional details
     */
    function recordSuspiciousActivity(
        string calldata roomId,
        address participant,
        SuspiciousActivityType activityType,
        string calldata details
    ) external roomActive(roomId) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");

        roomEvents[roomId].push(ProctorEvent({
            eventType: 5,
            participant: participant,
            data: details,
            timestamp: block.timestamp
        }));

        emit SuspiciousActivity(roomId, participant, activityType, block.timestamp);
    }

    /**
     * @notice Records recording started
     * @param roomId Room identifier
     * @param participant Wallet address of the participant being recorded
     */
    function recordRecordingStarted(
        string calldata roomId,
        address participant
    ) external roomActive(roomId) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");

        roomEvents[roomId].push(ProctorEvent({
            eventType: 6,
            participant: participant,
            data: "",
            timestamp: block.timestamp
        }));

        emit RecordingStarted(roomId, participant, block.timestamp);
    }

    /**
     * @notice Records recording stopped and optionally adds CID to exam result
     * @param roomId Room identifier
     * @param participant Wallet address of the participant
     * @param durationSecs Duration of recording in seconds
     * @param ipfsCid IPFS CID of the recorded content
     */
    function recordRecordingStopped(
        string calldata roomId,
        address participant,
        uint64 durationSecs,
        string calldata ipfsCid
    ) external roomExists(roomId) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");

        roomEvents[roomId].push(ProctorEvent({
            eventType: 7,
            participant: participant,
            data: ipfsCid,
            timestamp: block.timestamp
        }));

        // If participant has an exam result for this room, add the recording CID
        uint256 resultId = roomParticipantResultId[roomId][participant];
        if (resultId > 0 && bytes(ipfsCid).length > 0) {
            examResultRecordings[resultId].push(ipfsCid);
            examResults[resultId].updatedAt = block.timestamp;
            emit RecordingAdded(resultId, ipfsCid, block.timestamp);
        }

        emit RecordingStopped(roomId, participant, durationSecs, ipfsCid, block.timestamp);
    }

    /**
     * @notice Closes a room
     * @param roomId Room identifier
     * @param reason Reason for closing
     */
    function closeRoom(
        string calldata roomId,
        RoomCloseReason reason
    ) external roomActive(roomId) {
        rooms[roomId].status = RoomStatus.Closed;
        rooms[roomId].closedAt = block.timestamp;

        roomEvents[roomId].push(ProctorEvent({
            eventType: 8,
            participant: address(0),
            data: "",
            timestamp: block.timestamp
        }));

        emit RoomClosed(roomId, reason, block.timestamp);
    }

    /**
     * @notice Creates an exam result for a participant (typically a student)
     * @param roomId Room identifier
     * @param participant Wallet address of the participant
     * @param grade Grade out of 10000 (e.g., 8750 = 87.50%)
     * @param examName Name of the exam
     * @return resultId The ID of the created exam result
     */
    function createExamResult(
        string calldata roomId,
        address participant,
        uint256 grade,
        string calldata examName
    ) external roomExists(roomId) returns (uint256) {
        require(roomParticipants[roomId][participant].exists, "Participant not in room");
        require(roomParticipants[roomId][participant].role == Role.Student, "Not a student");
        require(grade <= 10000, "Grade must be <= 10000");
        require(roomParticipantResultId[roomId][participant] == 0, "Result already exists for this room");

        uint256 resultId = nextExamResultId++;

        examResults[resultId] = ExamResult({
            id: resultId,
            roomId: roomId,
            participant: participant,
            grade: grade,
            examName: examName,
            createdAt: block.timestamp,
            updatedAt: block.timestamp,
            nftMinted: false,
            exists: true
        });

        // Link result to participant
        participantExamResults[participant].push(resultId);
        roomParticipantResultId[roomId][participant] = resultId;

        emit ExamResultCreated(resultId, roomId, participant, grade, block.timestamp);

        return resultId;
    }

    /**
     * @notice Adds a recording CID to an existing exam result
     * @param resultId The exam result ID
     * @param ipfsCid IPFS CID of the recording
     */
    function addRecordingToResult(
        uint256 resultId,
        string calldata ipfsCid
    ) external examResultExists(resultId) {
        require(bytes(ipfsCid).length > 0, "CID cannot be empty");

        examResultRecordings[resultId].push(ipfsCid);
        examResults[resultId].updatedAt = block.timestamp;

        emit RecordingAdded(resultId, ipfsCid, block.timestamp);
    }

    /**
     * @notice Adds multiple recording CIDs to an existing exam result
     * @param resultId The exam result ID
     * @param ipfsCids Array of IPFS CIDs
     */
    function addRecordingsToResult(
        uint256 resultId,
        string[] calldata ipfsCids
    ) external examResultExists(resultId) {
        require(ipfsCids.length > 0, "No CIDs provided");

        for (uint256 i = 0; i < ipfsCids.length; i++) {
            if (bytes(ipfsCids[i]).length > 0) {
                examResultRecordings[resultId].push(ipfsCids[i]);
                emit RecordingAdded(resultId, ipfsCids[i], block.timestamp);
            }
        }
        examResults[resultId].updatedAt = block.timestamp;
    }

    /**
     * @notice Updates the grade of an exam result
     * @param resultId The exam result ID
     * @param newGrade New grade out of 10000
     */
    function updateExamResultGrade(
        uint256 resultId,
        uint256 newGrade
    ) external examResultExists(resultId) {
        require(newGrade <= 10000, "Grade must be <= 10000");
        require(!examResults[resultId].nftMinted, "Cannot update after NFT minted");

        examResults[resultId].grade = newGrade;
        examResults[resultId].updatedAt = block.timestamp;
    }

    /**
     * @notice Marks an NFT as minted for an exam result
     * @param resultId The exam result ID
     */
    function markNftMinted(uint256 resultId) external onlyOwner examResultExists(resultId) {
        require(!examResults[resultId].nftMinted, "NFT already minted");

        examResults[resultId].nftMinted = true;
        examResults[resultId].updatedAt = block.timestamp;

        ExamResult storage result = examResults[resultId];
        emit NftMinted(resultId, result.participant, result.roomId, block.timestamp);
    }

    // View functions

    /**
     * @notice Gets room information
     */
    function getRoomInfo(string calldata roomId) external view returns (
        address proctor,
        string memory proctorName,
        uint256 createdAt,
        uint256 closedAt,
        uint32 participantCount,
        RoomStatus status
    ) {
        require(rooms[roomId].exists, "Room not found");
        RoomInfo storage room = rooms[roomId];
        return (
            room.proctor,
            room.proctorName,
            room.createdAt,
            room.closedAt,
            room.participantCount,
            room.status
        );
    }

    /**
     * @notice Gets participant info for a room
     */
    function getParticipant(
        string calldata roomId,
        address participant
    ) external view returns (
        address wallet,
        string memory name,
        Role role,
        uint256 joinedAt,
        uint256 leftAt,
        uint256 examResultId
    ) {
        require(roomParticipants[roomId][participant].exists, "Participant not found");
        Participant storage p = roomParticipants[roomId][participant];
        uint256 resultId = roomParticipantResultId[roomId][participant];
        return (p.wallet, p.name, p.role, p.joinedAt, p.leftAt, resultId);
    }

    /**
     * @notice Gets all participant addresses for a room
     */
    function getRoomParticipants(string calldata roomId) external view returns (address[] memory) {
        return roomParticipantList[roomId];
    }

    /**
     * @notice Gets all rooms a participant has joined
     */
    function getParticipantRooms(address participant) external view returns (string[] memory) {
        return participantRooms[participant];
    }

    /**
     * @notice Gets all exam result IDs for a participant
     */
    function getParticipantExamResultIds(address participant) external view returns (uint256[] memory) {
        return participantExamResults[participant];
    }

    /**
     * @notice Gets the number of events for a room
     */
    function getEventCount(string calldata roomId) external view returns (uint256) {
        return roomEvents[roomId].length;
    }

    /**
     * @notice Gets events for a room with pagination
     */
    function getRoomEvents(
        string calldata roomId,
        uint256 offset,
        uint256 limit
    ) external view returns (ProctorEvent[] memory) {
        ProctorEvent[] storage events = roomEvents[roomId];
        uint256 total = events.length;

        if (offset >= total) {
            return new ProctorEvent[](0);
        }

        uint256 end = offset + limit;
        if (end > total) {
            end = total;
        }

        uint256 resultLength = end - offset;
        ProctorEvent[] memory result = new ProctorEvent[](resultLength);

        for (uint256 i = 0; i < resultLength; i++) {
            result[i] = events[offset + i];
        }

        return result;
    }

    /**
     * @notice Gets an exam result by ID
     */
    function getExamResult(uint256 resultId) external view examResultExists(resultId) returns (
        uint256 id,
        string memory roomId,
        address participant,
        uint256 grade,
        string memory examName,
        uint256 createdAt,
        uint256 updatedAt,
        bool nftMinted,
        uint256 recordingCount
    ) {
        ExamResult storage result = examResults[resultId];
        return (
            result.id,
            result.roomId,
            result.participant,
            result.grade,
            result.examName,
            result.createdAt,
            result.updatedAt,
            result.nftMinted,
            examResultRecordings[resultId].length
        );
    }

    /**
     * @notice Gets all recording CIDs for an exam result
     */
    function getExamResultRecordings(uint256 resultId) external view examResultExists(resultId) returns (string[] memory) {
        return examResultRecordings[resultId];
    }

    /**
     * @notice Gets exam result for a participant in a specific room
     */
    function getRoomParticipantExamResult(
        string calldata roomId,
        address participant
    ) external view returns (
        uint256 resultId,
        uint256 grade,
        string memory examName,
        uint256 createdAt,
        bool nftMinted,
        uint256 recordingCount
    ) {
        uint256 id = roomParticipantResultId[roomId][participant];
        require(id > 0, "No result found");

        ExamResult storage result = examResults[id];
        return (
            id,
            result.grade,
            result.examName,
            result.createdAt,
            result.nftMinted,
            examResultRecordings[id].length
        );
    }

    /**
     * @notice Gets students eligible for NFT (have results but no NFT minted)
     * @param roomId Room identifier
     */
    function getStudentsForNft(string calldata roomId) external view returns (address[] memory) {
        address[] storage participants = roomParticipantList[roomId];

        // First pass: count eligible students
        uint256 count = 0;
        for (uint256 i = 0; i < participants.length; i++) {
            address p = participants[i];
            uint256 resultId = roomParticipantResultId[roomId][p];
            if (resultId > 0 && !examResults[resultId].nftMinted) {
                count++;
            }
        }

        // Second pass: collect addresses
        address[] memory eligible = new address[](count);
        uint256 j = 0;
        for (uint256 i = 0; i < participants.length; i++) {
            address p = participants[i];
            uint256 resultId = roomParticipantResultId[roomId][p];
            if (resultId > 0 && !examResults[resultId].nftMinted) {
                eligible[j++] = p;
            }
        }

        return eligible;
    }

    /**
     * @notice Gets the total number of exam results
     */
    function getTotalExamResults() external view returns (uint256) {
        return nextExamResultId - 1;
    }
}
