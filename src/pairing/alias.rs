use rand::RngExt;

const ADJECTIVES: &[&str] = &[
    "bold", "brave", "bright", "calm", "clever", "cool", "daring", "dark", "deep", "eager", "fair",
    "fast", "fierce", "free", "fresh", "gentle", "grand", "happy", "keen", "kind", "light", "loud",
    "loyal", "lucky", "mighty", "noble", "proud", "pure", "quick", "quiet", "rapid", "sharp",
    "silent", "slim", "slow", "smart", "smooth", "stern", "still", "strong", "swift", "tall",
    "tough", "warm", "wild", "wise", "witty", "young", "zesty",
];

const COLORS: &[&str] = &[
    "amber",
    "azure",
    "beige",
    "black",
    "blue",
    "bronze",
    "brown",
    "coral",
    "crimson",
    "cyan",
    "gold",
    "gray",
    "green",
    "indigo",
    "ivory",
    "jade",
    "lavender",
    "lime",
    "magenta",
    "maroon",
    "mint",
    "navy",
    "olive",
    "orange",
    "peach",
    "pink",
    "purple",
    "red",
    "rose",
    "ruby",
    "salmon",
    "scarlet",
    "silver",
    "teal",
    "turquoise",
    "violet",
    "white",
    "yellow",
];

const ANIMALS: &[&str] = &[
    "badger",
    "bear",
    "bison",
    "boar",
    "cobra",
    "condor",
    "cougar",
    "crane",
    "crow",
    "deer",
    "eagle",
    "elk",
    "falcon",
    "fox",
    "gecko",
    "hawk",
    "heron",
    "jaguar",
    "koala",
    "lemur",
    "leopard",
    "lion",
    "lynx",
    "mink",
    "moose",
    "osprey",
    "otter",
    "owl",
    "panda",
    "panther",
    "parrot",
    "penguin",
    "puma",
    "raccoon",
    "raven",
    "rhino",
    "shark",
    "sloth",
    "snake",
    "stork",
    "tiger",
    "viper",
    "walrus",
    "whale",
    "wolf",
    "wolverine",
    "wombat",
    "yak",
    "zebra",
];

pub fn generate() -> String {
    let mut rng = rand::rng(); // not security-sentitve
    let adj = ADJECTIVES[rng.random_range(0..ADJECTIVES.len())];
    let color = COLORS[rng.random_range(0..COLORS.len())];
    let animal = ANIMALS[rng.random_range(0..ANIMALS.len())];
    format!("{adj}-{color}-{animal}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alias_format() {
        let alias = generate();
        let parts: Vec<&str> = alias.split('-').collect();
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_alias_not_empty() {
        let alias = generate();
        assert!(!alias.is_empty());
    }

    #[test]
    fn test_alias_words_are_known() {
        let alias = generate();
        let parts: Vec<&str> = alias.split('-').collect();
        assert!(ADJECTIVES.contains(&parts[0]));
        assert!(COLORS.contains(&parts[1]));
        assert!(ANIMALS.contains(&parts[2]));
    }

    #[test]
    fn test_aliases_are_unique() {
        // statistically near impossible to collide across 10 generations
        let aliases: Vec<String> = (0..10).map(|_| generate()).collect();
        let unique: std::collections::HashSet<&String> = aliases.iter().collect();
        assert_eq!(unique.len(), aliases.len());
    }
}
