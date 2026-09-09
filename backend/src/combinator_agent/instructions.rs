use super::latent::AnalysisKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionDomain {
    Sales,
    Teaching,
    Recruiting,
    General,
}

pub fn applicable(domain: InstructionDomain, kind: AnalysisKind) -> Result<&'static str, String> {
    let instruction = match (domain, kind) {
        (InstructionDomain::Sales, AnalysisKind::Template)
        | (InstructionDomain::Sales, AnalysisKind::PersonProfile) => {
            include_str!("../../assets/instructions/sales-v1.md")
        }
        (InstructionDomain::Teaching, _) => {
            include_str!("../../assets/instructions/teaching-v1.md")
        }
        (InstructionDomain::Recruiting, _) => {
            include_str!("../../assets/instructions/recruiting-v1.md")
        }
        (InstructionDomain::General, _) => {
            include_str!("../../assets/instructions/general-v1.md")
        }
    };
    if instruction.trim().is_empty() {
        return Err("applicable instruction is empty".to_owned());
    }
    Ok(instruction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_only_server_owned_versioned_instruction_files() {
        let instruction = applicable(InstructionDomain::Sales, AnalysisKind::Template).unwrap();
        assert!(instruction.contains("sales"));
        assert!(applicable(InstructionDomain::General, AnalysisKind::PersonProfile).is_ok());
    }
}
