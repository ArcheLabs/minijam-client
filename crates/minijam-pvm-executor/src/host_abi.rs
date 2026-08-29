//! MiniJAM SDK host-call identifiers.

/// Host calls defined by `service-toolchain/sdk/include/minijam/host.h`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiniJamHostCall {
    Gas = 0,
    Fetch = 1,
    Read = 3,
    Write = 4,
    New = 18,
    Transfer = 20,
    Yield = 25,
    Log = 100,
}

impl TryFrom<u32> for MiniJamHostCall {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Gas),
            1 => Ok(Self::Fetch),
            3 => Ok(Self::Read),
            4 => Ok(Self::Write),
            18 => Ok(Self::New),
            20 => Ok(Self::Transfer),
            25 => Ok(Self::Yield),
            100 => Ok(Self::Log),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MiniJamHostCall;

    const SDK_HOST_HEADER: &str =
        include_str!("../../../service-toolchain/sdk/include/minijam/host.h");

    #[test]
    fn sdk_hostcall_ids_match_header() {
        for (name, id) in [
            ("GAS", 0),
            ("FETCH", 1),
            ("READ", 3),
            ("WRITE", 4),
            ("NEW", 18),
            ("TRANSFER", 20),
            ("YIELD", 25),
            ("LOG", 100),
        ] {
            let declaration = format!("MINIJAM_HOST_{name} = {id}");
            assert!(
                SDK_HOST_HEADER.contains(&declaration),
                "SDK header is missing {declaration}"
            );
        }
    }

    #[test]
    fn all_sdk_hostcalls_are_decodable() {
        let ids = [
            (MiniJamHostCall::Gas, 0),
            (MiniJamHostCall::Fetch, 1),
            (MiniJamHostCall::Read, 3),
            (MiniJamHostCall::Write, 4),
            (MiniJamHostCall::New, 18),
            (MiniJamHostCall::Transfer, 20),
            (MiniJamHostCall::Yield, 25),
            (MiniJamHostCall::Log, 100),
        ];
        for (call, id) in ids {
            assert_eq!(MiniJamHostCall::try_from(id), Ok(call));
        }
    }
}
