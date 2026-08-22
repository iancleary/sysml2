package RequirementCapture {
    part def ControlledUnit;
    constraint def IsPowered;
    constraint def MeetsDeadline;
    constraint deadlineCheck : MeetsDeadline;

    requirement def <'REQ-CAPTURE-001'> BoundedCommandResponse {
        doc /* A controlled unit shall respond within its allocated deadline. */
        subject unit : ControlledUnit;
        assume constraint powered : IsPowered;
        require constraint deadlineCheck;
    }

    part controller : ControlledUnit;
    requirement boundedResponse : BoundedCommandResponse;
    satisfy boundedResponse by controller;
}
