//! Quick-add inline shorthand parser and grocery category classification.
//!
//! Parses natural language entries like:
//! - `"2 lbs apples"` -> qty: 2.0, unit: "lbs", name: "apples", category: "Produce"
//! - `"milk 1 gal"` -> qty: 1.0, unit: "gal", name: "milk", category: "Dairy & Eggs"
//! - `"6 eggs"` -> qty: 6.0, unit: None, name: "eggs", category: "Dairy & Eggs"
//! - `"sourdough bread"` -> qty: None, unit: None, name: "sourdough bread", category: "Bakery"
//! - `"2 gal organic milk @dairy"` -> qty: 2.0, unit: "gal", name: "organic milk", category: "dairy"

/// Materialized parsed grocery item ready for `ItemData` construction.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedItem {
    pub name: String,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub category: Option<String>,
}

/// Known quantity measurement units.
const KNOWN_UNITS: &[&str] = &[
    // Weight
    "lb", "lbs", "pound", "pounds", "oz", "ounce", "ounces", "g", "gram", "grams", "kg", "kilo",
    "kilos", // Volume
    "gal", "gallon", "gallons", "qt", "quart", "quarts", "pt", "pint", "pints", "cup", "cups", "l",
    "liter", "liters", "ml", // Packaging / Container
    "carton", "cartons", "bottle", "bottles", "can", "cans", "bag", "bags", "box", "boxes", "pack",
    "packs", "package", "packages", "bunch", "bunches", "head", "heads", "loaf", "loaves", "bar",
    "bars", "jar", "jars", "tub", "tubs", "dozen", "doz", "slice", "slices", "piece", "pieces",
];

/// Known grocery keyword taxonomy for automatic category suggestion.
const CATEGORY_MAP: &[(&str, &[&str])] = &[
    (
        "Produce",
        &[
            "apple",
            "apples",
            "banana",
            "bananas",
            "avocado",
            "avocados",
            "orange",
            "oranges",
            "lemon",
            "lemons",
            "lime",
            "limes",
            "tomato",
            "tomatoes",
            "onion",
            "onions",
            "garlic",
            "potato",
            "potatoes",
            "carrot",
            "carrots",
            "lettuce",
            "spinach",
            "kale",
            "broccoli",
            "pepper",
            "peppers",
            "cucumber",
            "cucumbers",
            "berry",
            "berries",
            "strawberry",
            "strawberries",
            "blueberry",
            "blueberries",
            "grape",
            "grapes",
            "celery",
            "squash",
            "zucchini",
            "asparagus",
            "cabbage",
        ],
    ),
    (
        "Dairy & Eggs",
        &[
            "milk",
            "cheese",
            "yogurt",
            "butter",
            "cream",
            "egg",
            "eggs",
            "cheddar",
            "mozzarella",
            "parmesan",
            "gouda",
            "brie",
            "oat milk",
            "almond milk",
            "soy milk",
            "sour cream",
            "cottage cheese",
            "heavy cream",
            "ghee",
            "kefir",
        ],
    ),
    (
        "Bakery",
        &[
            "bread",
            "bagel",
            "bagels",
            "bun",
            "buns",
            "roll",
            "rolls",
            "croissant",
            "croissants",
            "sourdough",
            "baguette",
            "tortilla",
            "tortillas",
            "pita",
            "muffin",
            "muffins",
            "pastry",
            "pastries",
            "toast",
            "brioche",
        ],
    ),
    (
        "Meat & Seafood",
        &[
            "chicken",
            "beef",
            "pork",
            "steak",
            "steaks",
            "bacon",
            "sausage",
            "sausages",
            "turkey",
            "salmon",
            "fish",
            "shrimp",
            "tuna",
            "ground beef",
            "ham",
            "lamb",
            "cod",
            "tilapia",
            "pork chops",
            "ribs",
        ],
    ),
    (
        "Pantry",
        &[
            "rice",
            "pasta",
            "noodle",
            "noodles",
            "flour",
            "sugar",
            "oil",
            "olive oil",
            "canola oil",
            "salt",
            "pepper",
            "spice",
            "spices",
            "sauce",
            "cereal",
            "oats",
            "oatmeal",
            "bean",
            "beans",
            "canned",
            "soup",
            "peanut butter",
            "jam",
            "jelly",
            "honey",
            "coffee",
            "tea",
            "chips",
            "cracker",
            "crackers",
            "nuts",
            "almonds",
            "walnuts",
            "cashews",
            "quinoa",
            "lentils",
            "vinegar",
            "soy sauce",
            "mayo",
            "mayonnaise",
            "ketchup",
            "mustard",
        ],
    ),
    (
        "Frozen",
        &[
            "ice cream",
            "pizza",
            "frozen",
            "frozen veggies",
            "frozen berries",
            "waffles",
            "dumplings",
            "popsicles",
            "french fries",
        ],
    ),
    (
        "Beverages",
        &[
            "juice",
            "soda",
            "water",
            "sparkling water",
            "seltzer",
            "beer",
            "wine",
            "cider",
            "kombucha",
            "lemonade",
            "tonic",
        ],
    ),
    (
        "Household",
        &[
            "soap",
            "detergent",
            "paper towel",
            "paper towels",
            "toilet paper",
            "trash bags",
            "sponge",
            "sponges",
            "shampoo",
            "conditioner",
            "toothpaste",
            "bleach",
            "foil",
            "plastic wrap",
            "batteries",
            "tissues",
        ],
    ),
];

/// Parses inline quantity, optional unit, item name, and suggests or overrides category.
pub fn parse_quick_add(input: &str) -> ParsedItem {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ParsedItem {
            name: String::new(),
            quantity: None,
            unit: None,
            category: None,
        };
    }

    // Extract explicit @category tag if present
    let mut explicit_category = None;
    let mut tokens: Vec<&str> = Vec::new();
    for token in trimmed.split_whitespace() {
        if let Some(cat) = token.strip_prefix('@') {
            if !cat.is_empty() {
                explicit_category = Some(cat.to_string());
                continue;
            }
        }
        tokens.push(token);
    }

    if tokens.is_empty() {
        return ParsedItem {
            name: trimmed.to_string(),
            quantity: None,
            unit: None,
            category: explicit_category.or_else(|| suggest_category(trimmed)),
        };
    }

    // 1. Try leading quantity (e.g., "2 lbs apples", "2.5 kg bananas", "6 eggs")
    if let Some(qty) = parse_quantity_token(tokens[0]) {
        if tokens.len() > 1 {
            // Check if token[1] is a known unit
            let unit_candidate = tokens[1].to_lowercase();
            if KNOWN_UNITS.contains(&unit_candidate.as_str()) && tokens.len() > 2 {
                let name = tokens[2..].join(" ");
                let category = explicit_category.clone().or_else(|| suggest_category(&name));
                return ParsedItem {
                    name,
                    quantity: Some(qty),
                    unit: Some(unit_candidate),
                    category,
                };
            } else {
                // No unit, remainder is item name
                let name = tokens[1..].join(" ");
                let category = explicit_category.clone().or_else(|| suggest_category(&name));
                return ParsedItem {
                    name,
                    quantity: Some(qty),
                    unit: None,
                    category,
                };
            }
        }
    }

    // 2. Try trailing quantity (e.g., "milk 1 gal", "apples 3 lbs", "eggs 12")
    if tokens.len() >= 3 {
        let last_idx = tokens.len() - 1;
        let unit_candidate = tokens[last_idx].to_lowercase();
        if KNOWN_UNITS.contains(&unit_candidate.as_str()) {
            if let Some(qty) = parse_quantity_token(tokens[last_idx - 1]) {
                let name = tokens[..last_idx - 1].join(" ");
                let category = explicit_category.clone().or_else(|| suggest_category(&name));
                return ParsedItem {
                    name,
                    quantity: Some(qty),
                    unit: Some(unit_candidate),
                    category,
                };
            }
        }
    }

    if tokens.len() >= 2 {
        let last_idx = tokens.len() - 1;
        if let Some(qty) = parse_quantity_token(tokens[last_idx]) {
            let name = tokens[..last_idx].join(" ");
            let category = explicit_category.clone().or_else(|| suggest_category(&name));
            return ParsedItem {
                name,
                quantity: Some(qty),
                unit: None,
                category,
            };
        }
    }

    // 3. Fallback: Entire input is item name
    let name = tokens.join(" ");
    let category = explicit_category.or_else(|| suggest_category(&name));
    ParsedItem {
        name,
        quantity: None,
        unit: None,
        category,
    }
}

/// Parses a number token, supporting integers, decimals, and simple fractions (e.g. "1/2").
fn parse_quantity_token(token: &str) -> Option<f64> {
    if let Ok(val) = token.parse::<f64>() {
        if val > 0.0 && val.is_finite() {
            return Some((val * 100.0).round() / 100.0);
        }
    }

    // Support fraction notation like "1/2", "1/4", "3/4"
    if let Some((num_str, den_str)) = token.split_once('/') {
        if let (Ok(num), Ok(den)) = (num_str.parse::<f64>(), den_str.parse::<f64>()) {
            if den > 0.0 && num > 0.0 {
                let val = num / den;
                return Some((val * 100.0).round() / 100.0);
            }
        }
    }

    None
}

/// Suggests a category based on matching keywords in item name (case-insensitive).
pub fn suggest_category(item_name: &str) -> Option<String> {
    let lower = item_name.to_lowercase();
    for &(category, keywords) in CATEGORY_MAP {
        for &kw in keywords {
            // Match word boundaries or exact substring
            if lower.contains(kw) {
                return Some(category.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_leading_qty_and_unit() {
        let parsed = parse_quick_add("2 lbs honeycrisp apples");
        assert_eq!(parsed.name, "honeycrisp apples");
        assert_eq!(parsed.quantity, Some(2.0));
        assert_eq!(parsed.unit.as_deref(), Some("lbs"));
        assert_eq!(parsed.category.as_deref(), Some("Produce"));
    }

    #[test]
    fn test_parse_leading_qty_no_unit() {
        let parsed = parse_quick_add("6 avocados");
        assert_eq!(parsed.name, "avocados");
        assert_eq!(parsed.quantity, Some(6.0));
        assert_eq!(parsed.unit, None);
        assert_eq!(parsed.category.as_deref(), Some("Produce"));
    }

    #[test]
    fn test_parse_trailing_qty_and_unit() {
        let parsed = parse_quick_add("whole milk 1 gal");
        assert_eq!(parsed.name, "whole milk");
        assert_eq!(parsed.quantity, Some(1.0));
        assert_eq!(parsed.unit.as_deref(), Some("gal"));
        assert_eq!(parsed.category.as_deref(), Some("Dairy & Eggs"));
    }

    #[test]
    fn test_parse_explicit_category_tag() {
        let parsed = parse_quick_add("2 gal organic milk @dairy");
        assert_eq!(parsed.name, "organic milk");
        assert_eq!(parsed.quantity, Some(2.0));
        assert_eq!(parsed.unit.as_deref(), Some("gal"));
        assert_eq!(parsed.category.as_deref(), Some("dairy"));
    }

    #[test]
    fn test_parse_fraction_quantity() {
        let parsed = parse_quick_add("1/2 lb butter");
        assert_eq!(parsed.name, "butter");
        assert_eq!(parsed.quantity, Some(0.5));
        assert_eq!(parsed.unit.as_deref(), Some("lb"));
        assert_eq!(parsed.category.as_deref(), Some("Dairy & Eggs"));
    }

    #[test]
    fn test_parse_plain_item_name() {
        let parsed = parse_quick_add("organic bananas");
        assert_eq!(parsed.name, "organic bananas");
        assert_eq!(parsed.quantity, None);
        assert_eq!(parsed.unit, None);
        assert_eq!(parsed.category.as_deref(), Some("Produce"));
    }
}
