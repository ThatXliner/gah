use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub index: usize,
    pub anchor: String,
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub lines: Vec<DiffLine>,
    pub function_context: Option<String>,
}

impl Hunk {
    pub fn additions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l, DiffLine::Add(_)))
            .count()
    }

    pub fn deletions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l, DiffLine::Remove(_)))
            .count()
    }

    pub fn content(&self) -> String {
        self.lines
            .iter()
            .map(|l| match l {
                DiffLine::Context(s) => format!(" {s}"),
                DiffLine::Add(s) => format!("+{s}"),
                DiffLine::Remove(s) => format!("-{s}"),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn new_end(&self) -> u32 {
        self.new_start + self.new_count.saturating_sub(1)
    }

    /// Recompute old/new counts from the current `lines`, leaving the start
    /// positions untouched, and rebuild the `@@` header to match. Used after
    /// trimming a hunk to a line subset.
    fn recount(&mut self) {
        self.old_count = self
            .lines
            .iter()
            .filter(|l| matches!(l, DiffLine::Context(_) | DiffLine::Remove(_)))
            .count() as u32;
        self.new_count = self
            .lines
            .iter()
            .filter(|l| matches!(l, DiffLine::Context(_) | DiffLine::Add(_)))
            .count() as u32;

        let fmt = |start: u32, count: u32| {
            if count == 1 {
                format!("{start}")
            } else {
                format!("{start},{count}")
            }
        };
        let ctx = self
            .function_context
            .as_deref()
            .map(|c| format!(" {c}"))
            .unwrap_or_default();
        self.header = format!(
            "@@ -{} +{} @@{ctx}",
            fmt(self.old_start, self.old_count),
            fmt(self.new_start, self.new_count),
        );
    }

    /// Restrict this hunk to only the changes whose new-file line numbers fall
    /// within any of `ranges` (inclusive). Changes outside every range are
    /// neutralized: a replacement keeps its old line (its removal demotes to
    /// context, its addition drops), a pure insertion drops, and a pure
    /// deletion drops (its old line stays). A result with no remaining change
    /// yields `None`.
    ///
    /// This is the non-interactive equivalent of `git add -p`'s edit mode: it
    /// lets a single hunk be staged piecewise. Trimming can leave few or zero
    /// context lines; a zero-context result must be applied with
    /// `--unidiff-zero` (see `apply::apply_hunks`).
    pub fn restrict_to_lines(&self, ranges: &[(u32, u32)]) -> Option<Hunk> {
        // Git emits a replacement as a run of removals followed by a run of
        // additions (e.g. `-b -c -d +B +C +D`), so a remove and the add that
        // replaces it are NOT adjacent in `self.lines`. To stage by new-file
        // line we must pair them positionally: removal *i* of a run lines up
        // with addition *i*, both occupying new-file line `new_cursor + i`.
        //
        // We walk the hunk, buffering each remove/add run, and flush the run as
        // interleaved (remove, add) pairs in new-file order. For each pair we
        // decide by its new-file line: in range -> keep the change; out of
        // range -> leave that line untouched in the staged result (how depends
        // on the pair's kind; see `flush`).
        let mut new_lines = Vec::with_capacity(self.lines.len());
        let mut new_cursor = self.new_start;
        let mut kept_change = false;

        let mut removes: Vec<&String> = Vec::new();
        let mut adds: Vec<&String> = Vec::new();

        // Flush a buffered replacement run as positional pairs.
        let flush = |removes: &mut Vec<&String>,
                     adds: &mut Vec<&String>,
                     new_cursor: &mut u32,
                     new_lines: &mut Vec<DiffLine>,
                     kept_change: &mut bool| {
            let pairs = removes.len().max(adds.len());
            for i in 0..pairs {
                let line = *new_cursor;
                let in_range = ranges
                    .iter()
                    .any(|(start, end)| line >= *start && line <= *end);
                let rem = removes.get(i);
                let add = adds.get(i);
                // Classify the position by what occupies new-file line `line`:
                //   replacement  (rem + add) -> occupies the line
                //   pure insert  (add only)  -> occupies the line
                //   pure delete  (rem only)  -> occupies NO new-file line
                // Only positions that occupy a new-file line advance the cursor.
                if in_range {
                    if let Some(r) = rem {
                        new_lines.push(DiffLine::Remove((*r).clone()));
                        *kept_change = true;
                    }
                    if let Some(a) = add {
                        new_lines.push(DiffLine::Add((*a).clone()));
                        *kept_change = true;
                    }
                } else if add.is_some() {
                    // Out-of-range position that occupies a new-file line:
                    // a replacement demotes its removal to context (the old
                    // line survives unchanged); a pure insertion is dropped
                    // (it isn't in the old file, so cannot become context).
                    if let Some(r) = rem {
                        new_lines.push(DiffLine::Context((*r).clone()));
                    }
                }
                // else: out-of-range pure deletion -> drop entirely. It occupies
                // no new-file line, so the cursor must NOT advance, or later
                // in-range changes would be measured against the wrong line.

                if add.is_some() {
                    *new_cursor += 1;
                }
            }
            removes.clear();
            adds.clear();
        };

        for line in &self.lines {
            match line {
                DiffLine::Remove(s) => removes.push(s),
                DiffLine::Add(s) => adds.push(s),
                DiffLine::Context(s) => {
                    flush(
                        &mut removes,
                        &mut adds,
                        &mut new_cursor,
                        &mut new_lines,
                        &mut kept_change,
                    );
                    new_lines.push(DiffLine::Context(s.clone()));
                    new_cursor += 1;
                }
            }
        }
        flush(
            &mut removes,
            &mut adds,
            &mut new_cursor,
            &mut new_lines,
            &mut kept_change,
        );

        if !kept_change {
            return None;
        }

        let mut hunk = Hunk {
            index: self.index,
            anchor: String::new(),
            header: String::new(),
            old_start: self.old_start,
            old_count: 0,
            new_start: self.new_start,
            new_count: 0,
            lines: new_lines,
            function_context: self.function_context.clone(),
        };
        hunk.recount();
        hunk.anchor = Hunk::compute_anchor(&hunk.content());
        Some(hunk)
    }

    pub fn compute_anchor(content: &str) -> String {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let hash = hasher.finish();

        // Single-token anchors (verified against cl100k_base/o200k_base tokenizers)
        // Each word is exactly 1 token, giving us 1024 unique anchors
        const ANCHORS: &[&str] = &[
            "Abandon",
            "Ability",
            "Abroad",
            "Absence",
            "Absolute",
            "Abstract",
            "Abuse",
            "Academy",
            "Accent",
            "Accept",
            "Access",
            "Accident",
            "Account",
            "Accuracy",
            "Accurate",
            "Achieve",
            "Acid",
            "Acquire",
            "Across",
            "Action",
            "Active",
            "Actual",
            "Adapt",
            "Addition",
            "Address",
            "Adequate",
            "Adjust",
            "Admin",
            "Admiral",
            "Adopt",
            "Adult",
            "Advance",
            "Adventure",
            "Advice",
            "Advocate",
            "Affair",
            "Affect",
            "Afford",
            "Africa",
            "Agency",
            "Agenda",
            "Agent",
            "Aggregate",
            "Agree",
            "Agreement",
            "Ahead",
            "Aircraft",
            "Airport",
            "Album",
            "Alcohol",
            "Alert",
            "Algebra",
            "Alien",
            "Align",
            "Alive",
            "Alliance",
            "Allocate",
            "Allow",
            "Alloy",
            "Alpha",
            "Already",
            "Alter",
            "Alternate",
            "Although",
            "Altitude",
            "Aluminum",
            "Always",
            "Amateur",
            "Amazing",
            "Amazon",
            "Ambient",
            "Ambition",
            "Amendment",
            "America",
            "Among",
            "Amount",
            "Amplifier",
            "Analysis",
            "Analyst",
            "Analyze",
            "Ancestor",
            "Anchor",
            "Ancient",
            "Android",
            "Angel",
            "Anger",
            "Angle",
            "Animal",
            "Animate",
            "Ankle",
            "Announce",
            "Annual",
            "Anonymous",
            "Answer",
            "Antenna",
            "Antique",
            "Anxiety",
            "Apache",
            "Apart",
            "Apartment",
            "Apex",
            "Apology",
            "Apparent",
            "Appeal",
            "Appear",
            "Appetite",
            "Apple",
            "Apply",
            "Appoint",
            "Approach",
            "Approve",
            "Aqua",
            "Arabic",
            "Arcade",
            "Arch",
            "Archive",
            "Arctic",
            "Arena",
            "Argue",
            "Argument",
            "Arise",
            "Armor",
            "Around",
            "Arrange",
            "Array",
            "Arrest",
            "Arrival",
            "Arrow",
            "Arsenal",
            "Article",
            "Artifact",
            "Artist",
            "Artwork",
            "Aside",
            "Aspect",
            "Assault",
            "Assert",
            "Assess",
            "Asset",
            "Assign",
            "Assist",
            "Associate",
            "Assume",
            "Assure",
            "Asteroid",
            "Athlete",
            "Atlanta",
            "Atlantic",
            "Atlas",
            "Atmosphere",
            "Atom",
            "Atomic",
            "Attach",
            "Attack",
            "Attempt",
            "Attend",
            "Attention",
            "Attitude",
            "Attorney",
            "Attract",
            "Auction",
            "Audience",
            "Audio",
            "Audit",
            "August",
            "Austin",
            "Australia",
            "Authentic",
            "Author",
            "Authority",
            "Auto",
            "Autumn",
            "Avatar",
            "Avenue",
            "Average",
            "Aviation",
            "Avoid",
            "Award",
            "Awesome",
            "Awful",
            "Axis",
            "Azure",
            "Backup",
            "Bacon",
            "Badge",
            "Balance",
            "Balloon",
            "Baltic",
            "Bamboo",
            "Banana",
            "Bandwidth",
            "Bangkok",
            "Banking",
            "Banner",
            "Barbecue",
            "Bargain",
            "Barrier",
            "Baseball",
            "Baseline",
            "Basement",
            "Basic",
            "Basin",
            "Basket",
            "Battery",
            "Battle",
            "Bavaria",
            "Beach",
            "Beacon",
            "Bearing",
            "Beast",
            "Beauty",
            "Become",
            "Bedroom",
            "Before",
            "Begin",
            "Behavior",
            "Behind",
            "Belief",
            "Belong",
            "Below",
            "Benchmark",
            "Beneath",
            "Benefit",
            "Berlin",
            "Besides",
            "Beta",
            "Better",
            "Between",
            "Beyond",
            "Bible",
            "Bicycle",
            "Biden",
            "Billion",
            "Binary",
            "Binding",
            "Biology",
            "Bitmap",
            "Bizarre",
            "Blade",
            "Blanket",
            "Blast",
            "Blend",
            "Blessing",
            "Blind",
            "Blockchain",
            "Blossom",
            "Blueprint",
            "Bluetooth",
            "Board",
            "Boating",
            "Bobby",
            "Bodily",
            "Boeing",
            "Boiling",
            "Bolivia",
            "Bombing",
            "Bonding",
            "Bonus",
            "Booking",
            "Boolean",
            "Boost",
            "Border",
            "Bosnia",
            "Boston",
            "Botanic",
            "Bother",
            "Bottle",
            "Bottom",
            "Boulder",
            "Boundary",
            "Boutique",
            "Boxing",
            "Bracket",
            "Brain",
            "Branch",
            "Brand",
            "Brave",
            "Brazil",
            "Breach",
            "Bread",
            "Break",
            "Breast",
            "Breath",
            "Breed",
            "Brick",
            "Bridge",
            "Brief",
            "Bright",
            "Bring",
            "Britain",
            "Broad",
            "Broadway",
            "Broker",
            "Bronze",
            "Brother",
            "Brown",
            "Browser",
            "Brutal",
            "Bubble",
            "Bucket",
            "Budapest",
            "Buddha",
            "Budget",
            "Buffer",
            "Build",
            "Builder",
            "Building",
            "Bulgaria",
            "Bullet",
            "Bulletin",
            "Bundle",
            "Bureau",
            "Burger",
            "Burial",
            "Burma",
            "Burning",
            "Burst",
            "Burton",
            "Business",
            "Butler",
            "Butter",
            "Button",
            "Buyer",
            "Bypass",
            "Cabinet",
            "Cable",
            "Cache",
            "Caesar",
            "Cairo",
            "Calcium",
            "Calculate",
            "Calendar",
            "Calgary",
            "Caliber",
            "California",
            "Caller",
            "Calling",
            "Cambodia",
            "Cambridge",
            "Camera",
            "Campaign",
            "Camping",
            "Campus",
            "Canada",
            "Canadian",
            "Canal",
            "Cancel",
            "Cancer",
            "Candle",
            "Cannon",
            "Canvas",
            "Canyon",
            "Capable",
            "Capacity",
            "Capital",
            "Captain",
            "Caption",
            "Capture",
            "Carbon",
            "Cardiac",
            "Cardinal",
            "Career",
            "Careful",
            "Cargo",
            "Caribbean",
            "Carlos",
            "Carnival",
            "Caroline",
            "Carpet",
            "Carrier",
            "Carroll",
            "Cartoon",
            "Cascade",
            "Casino",
            "Castle",
            "Castro",
            "Casual",
            "Catalog",
            "Catalyst",
            "Category",
            "Cathedral",
            "Catholic",
            "Cattle",
            "Caught",
            "Caution",
            "Cavalry",
            "Ceiling",
            "Celebrate",
            "Celebrity",
            "Cellular",
            "Celtic",
            "Cement",
            "Cemetery",
            "Census",
            "Center",
            "Central",
            "Century",
            "Ceramic",
            "Ceremony",
            "Certain",
            "Certificate",
            "Challenge",
            "Chamber",
            "Champion",
            "Championship",
            "Chance",
            "Change",
            "Channel",
            "Chapter",
            "Character",
            "Charge",
            "Charity",
            "Charles",
            "Charlie",
            "Charlotte",
            "Charm",
            "Charter",
            "Chase",
            "Cheap",
            "Check",
            "Cheese",
            "Chelsea",
            "Chemical",
            "Chemistry",
            "Chennai",
            "Cherry",
            "Chess",
            "Chester",
            "Chicago",
            "Chicken",
            "Chief",
            "Child",
            "Chile",
            "China",
            "Chinese",
            "Chip",
            "Chocolate",
            "Choice",
            "Choir",
            "Choose",
            "Chord",
            "Christ",
            "Christian",
            "Christmas",
            "Chrome",
            "Chronicle",
            "Church",
            "Cinema",
            "Circle",
            "Circuit",
            "Circular",
            "Circus",
            "Citation",
            "Citizen",
            "Civil",
            "Civilian",
            "Claim",
            "Claire",
            "Clarity",
            "Clarke",
            "Clash",
            "Classic",
            "Classroom",
            "Clause",
            "Clayton",
            "Clean",
            "Clear",
            "Clerk",
            "Cleveland",
            "Click",
            "Client",
            "Cliff",
            "Climate",
            "Climb",
            "Clinic",
            "Clinical",
            "Clinton",
            "Clock",
            "Clone",
            "Close",
            "Closer",
            "Closing",
            "Cloth",
            "Cloud",
            "Cluster",
            "Coach",
            "Coal",
            "Coalition",
            "Coast",
            "Coastal",
            "Coating",
            "Cocktail",
            "Coconut",
            "Coding",
            "Coffee",
            "Cognitive",
            "Cohen",
            "Coincidence",
            "Cold",
            "Coleman",
            "Collapse",
            "Collar",
            "Collect",
            "Collection",
            "Collector",
            "College",
            "Collins",
            "Colonial",
            "Colony",
            "Color",
            "Colorado",
            "Columbia",
            "Columbus",
            "Column",
            "Combat",
            "Combine",
            "Comedy",
            "Comfort",
            "Comic",
            "Coming",
            "Command",
            "Commander",
            "Comment",
            "Commerce",
            "Commercial",
            "Commission",
            "Commit",
            "Committee",
            "Commodity",
            "Common",
            "Communicate",
            "Community",
            "Compact",
            "Companion",
            "Company",
            "Compare",
            "Compass",
            "Compatible",
            "Compel",
            "Compensate",
            "Compete",
            "Competition",
            "Competitive",
            "Competitor",
            "Compile",
            "Complain",
            "Complaint",
            "Complete",
            "Complex",
            "Compliance",
            "Complicate",
            "Component",
            "Compose",
            "Composite",
            "Compound",
            "Comprehensive",
            "Comprise",
            "Compromise",
            "Compute",
            "Computer",
            "Conceive",
            "Concentrate",
            "Concept",
            "Concern",
            "Concert",
            "Conclude",
            "Conclusion",
            "Concrete",
            "Condition",
            "Conduct",
            "Conductor",
            "Conference",
            "Confidence",
            "Confident",
            "Configuration",
            "Configure",
            "Confirm",
            "Conflict",
            "Conform",
            "Confront",
            "Confuse",
            "Congo",
            "Congress",
            "Connect",
            "Connecticut",
            "Connection",
            "Connor",
            "Conscious",
            "Consensus",
            "Consent",
            "Consequence",
            "Conservation",
            "Conservative",
            "Consider",
            "Consist",
            "Console",
            "Conspiracy",
            "Constant",
            "Constitute",
            "Constitution",
            "Constraint",
            "Construct",
            "Construction",
            "Consult",
            "Consultant",
            "Consumer",
            "Consumption",
            "Contact",
            "Contain",
            "Container",
            "Contemporary",
            "Content",
            "Contest",
            "Context",
            "Continent",
            "Continue",
            "Continuous",
            "Contract",
            "Contractor",
            "Contrast",
            "Contribute",
            "Contribution",
            "Control",
            "Controller",
            "Controversy",
            "Convenience",
            "Convention",
            "Conversation",
            "Conversion",
            "Convert",
            "Conviction",
            "Convince",
            "Cookie",
            "Cooking",
            "Cooper",
            "Cooperate",
            "Coordinate",
            "Copenhagen",
            "Copper",
            "Copyright",
            "Coral",
            "Corner",
            "Cornwall",
            "Corona",
            "Corporate",
            "Corporation",
            "Correct",
            "Correction",
            "Correlate",
            "Correspond",
            "Corridor",
            "Corrupt",
            "Cosmic",
            "Costa",
            "Costume",
            "Cottage",
            "Cotton",
            "Council",
            "Counsel",
            "Counter",
            "Counting",
            "Country",
            "Countryside",
            "County",
            "Couple",
            "Courage",
            "Course",
            "Court",
            "Courtesy",
            "Cousin",
            "Cover",
            "Coverage",
            "Cowboy",
            "Crack",
            "Craft",
            "Craig",
            "Crash",
            "Crater",
            "Crazy",
            "Cream",
            "Create",
            "Creation",
            "Creative",
            "Creator",
            "Creature",
            "Credit",
            "Creek",
            "Crew",
            "Cricket",
            "Crime",
            "Criminal",
            "Crisis",
            "Criteria",
            "Critic",
            "Critical",
            "Criticism",
            "Croatia",
            "Cross",
            "Crowd",
            "Crown",
            "Crucial",
            "Crude",
            "Cruise",
            "Crystal",
            "Cuba",
            "Cuban",
            "Cubic",
            "Cuisine",
            "Cultural",
            "Culture",
            "Cumberland",
            "Curious",
            "Currency",
            "Current",
            "Curriculum",
            "Curtis",
            "Curve",
            "Custom",
            "Customer",
            "Customs",
            "Cutting",
            "Cyber",
            "Cycle",
            "Cylinder",
            "Cyprus",
            "Czech",
            "Daddy",
            "Daily",
            "Dairy",
            "Dakota",
            "Dallas",
            "Damage",
            "Damascus",
            "Dance",
            "Dancing",
            "Danger",
            "Daniel",
            "Danish",
            "Danny",
            "Database",
            "Dating",
            "Daughter",
            "David",
            "Davis",
            "Dawn",
            "Dealer",
            "Dealing",
            "Dean",
            "Death",
            "Debate",
            "Debris",
            "Debt",
            "Debut",
            "Decade",
            "Decay",
            "December",
            "Decent",
            "Decide",
            "Decision",
            "Declare",
            "Decline",
            "Decor",
            "Decrease",
            "Dedicate",
            "Deed",
            "Deemed",
            "Deep",
            "Deeper",
            "Default",
            "Defeat",
            "Defend",
            "Defendant",
            "Defense",
            "Defensive",
            "Define",
            "Definite",
            "Definition",
            "Degree",
            "Deity",
            "Delay",
            "Delegate",
            "Delete",
            "Delhi",
            "Deliberate",
            "Delicate",
            "Deliver",
            "Delivery",
            "Delta",
            "Demand",
            "Democracy",
            "Democrat",
            "Democratic",
            "Demographic",
            "Demonstrate",
            "Denmark",
            "Dennis",
            "Dense",
            "Density",
            "Dental",
            "Denver",
            "Deny",
            "Depart",
            "Department",
            "Depend",
            "Dependent",
            "Depict",
            "Deploy",
            "Deposit",
            "Depot",
            "Depression",
            "Depth",
            "Deputy",
            "Derive",
            "Descend",
            "Describe",
            "Description",
            "Desert",
            "Deserve",
            "Design",
            "Designer",
            "Desire",
            "Desktop",
            "Despite",
            "Destination",
            "Destiny",
            "Destroy",
            "Detail",
            "Detailed",
            "Detect",
            "Detective",
            "Detector",
            "Determine",
            "Detroit",
            "Develop",
            "Developer",
            "Development",
            "Device",
            "Devil",
            "Devote",
            "Diabetes",
            "Diagnose",
            "Diagnostic",
            "Diagram",
            "Dialect",
            "Dialog",
            "Dialogue",
            "Diamond",
            "Diana",
            "Diary",
            "Dickinson",
            "Dictionary",
            "Diego",
            "Diesel",
            "Dietary",
            "Differ",
            "Difference",
            "Different",
            "Differential",
            "Difficult",
            "Difficulty",
            "Digital",
            "Dignity",
            "Dilemma",
            "Dimension",
            "Dining",
            "Dinner",
            "Dinosaur",
            "Diploma",
            "Diplomat",
            "Direct",
            "Direction",
            "Director",
            "Directory",
            "Dirty",
            "Disable",
            "Disaster",
            "Discard",
            "Discipline",
            "Disclose",
            "Disco",
            "Discount",
            "Discourse",
            "Discover",
            "Discovery",
            "Discrete",
            "Discrimination",
            "Discuss",
            "Discussion",
            "Disease",
            "Dish",
            "Disk",
            "Disney",
            "Disorder",
            "Dispatch",
            "Display",
            "Disposal",
            "Dispute",
            "Distance",
            "Distant",
            "Distinct",
            "Distinction",
            "Distinguish",
            "Distribute",
            "Distribution",
            "District",
            "Diverse",
            "Diversity",
            "Divide",
            "Divine",
            "Division",
            "Divorce",
            "Dixon",
            "Doctor",
            "Doctrine",
            "Document",
            "Documentary",
            "Documentation",
            "Dollar",
            "Dolphin",
            "Domain",
            "Dome",
            "Domestic",
            "Dominant",
            "Dominate",
            "Dominican",
            "Donate",
            "Donation",
            "Donor",
            "Doom",
            "Door",
            "Dora",
            "Dose",
            "Double",
            "Doubt",
            "Douglas",
            "Dover",
            "Download",
            "Downtown",
            "Dozen",
            "Draft",
            "Dragon",
            "Drama",
            "Dramatic",
            "Draw",
            "Drawing",
            "Dream",
            "Dress",
            "Drift",
            "Drill",
            "Drink",
            "Drinking",
            "Drive",
            "Driver",
            "Driving",
            "Drop",
            "Drought",
            "Drum",
            "Dubai",
            "Dublin",
            "Duck",
            "Duke",
            "Dummy",
            "Dump",
            "Duncan",
            "Dundee",
            "Duration",
            "During",
            "Durham",
            "Dutch",
            "Duty",
            "Dwelling",
            "Dynamic",
            "Dynasty",
            "Eagle",
            "Early",
            "Earth",
            "Earthquake",
            "Easily",
            "East",
            "Easter",
            "Eastern",
            "Easy",
            "Eating",
            "Echo",
            "Eclipse",
            "Ecology",
            "Economic",
            "Economy",
            "Ecosystem",
            "Ecuador",
            "Eddie",
            "Edge",
            "Edinburgh",
            "Edit",
            "Edition",
            "Editor",
            "Editorial",
            "Edmund",
            "Educate",
            "Education",
            "Educational",
            "Educator",
            "Edward",
            "Effect",
            "Effective",
            "Efficiency",
            "Efficient",
            "Effort",
            "Egypt",
            "Egyptian",
            "Einstein",
            "Either",
            "Elder",
            "Elderly",
            "Eleanor",
            "Elect",
            "Election",
            "Electric",
            "Electrical",
            "Electricity",
            "Electron",
            "Electronic",
            "Element",
            "Elementary",
            "Elephant",
            "Elevation",
            "Elite",
            "Elizabeth",
            "Ellen",
            "Elliott",
            "Ellis",
            "Elsewhere",
            "Email",
            "Embark",
            "Embassy",
            "Embed",
            "Embrace",
            "Emerge",
            "Emergency",
            "Emerging",
            "Emily",
            "Emission",
            "Emit",
            "Emma",
            "Emotion",
            "Emotional",
            "Emperor",
            "Emphasis",
            "Empire",
            "Empirical",
            "Employ",
            "Employee",
            "Employer",
            "Employment",
            "Empty",
            "Enable",
            "Encounter",
            "Encourage",
            "Encyclopedia",
            "Ending",
            "Endless",
            "Endorse",
            "Enemy",
            "Energy",
            "Enforce",
            "Engage",
            "Engine",
            "Engineer",
            "Engineering",
            "England",
            "English",
            "Enhance",
            "Enjoy",
            "Enormous",
            "Enough",
            "Enquiry",
            "Ensure",
            "Enter",
        ];

        let idx = (hash as usize) % ANCHORS.len();
        ANCHORS[idx].to_string()
    }
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub path: PathBuf,
    pub hunks: Vec<Hunk>,
    pub diff_header: String,
}

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("invalid hunk header: {0}")]
    #[allow(dead_code)]
    InvalidHunkHeader(String),
    #[error("no files in diff")]
    #[allow(dead_code)]
    NoFiles,
}

pub fn parse_diff(diff_output: &str) -> Result<Vec<DiffFile>, ParseError> {
    let mut files = Vec::new();
    let file_re = Regex::new(r"^diff --git a/(.+) b/(.+)$").unwrap();
    let hunk_re = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$").unwrap();

    let file_chunks: Vec<&str> = diff_output.split("\ndiff --git ").collect();

    for (i, chunk) in file_chunks.iter().enumerate() {
        let chunk = if i == 0 {
            chunk.strip_prefix("diff --git ").unwrap_or(chunk)
        } else {
            chunk
        };

        if chunk.trim().is_empty() {
            continue;
        }

        let full_chunk = format!("diff --git {chunk}");
        let lines: Vec<&str> = full_chunk.lines().collect();

        if lines.is_empty() {
            continue;
        }

        let Some(caps) = file_re.captures(lines[0]) else {
            continue;
        };

        let path = PathBuf::from(&caps[2]);

        // Check for binary file
        if lines.iter().any(|l| l.starts_with("Binary files")) {
            continue;
        }

        // Find diff header (everything before first hunk)
        let mut header_end = lines.len();
        for (j, line) in lines.iter().enumerate() {
            if line.starts_with("@@") {
                header_end = j;
                break;
            }
        }

        let diff_header = lines[..header_end].join("\n");

        // Parse hunks
        let mut hunks = Vec::new();
        let mut current_hunk: Option<Hunk> = None;
        let mut hunk_index = 0;

        for line in &lines[header_end..] {
            if let Some(caps) = hunk_re.captures(line) {
                if let Some(h) = current_hunk.take() {
                    hunks.push(h);
                }

                hunk_index += 1;
                let old_start: u32 = caps[1].parse().unwrap_or(0);
                let old_count: u32 = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap_or(1));
                let new_start: u32 = caps[3].parse().unwrap_or(0);
                let new_count: u32 = caps.get(4).map_or(1, |m| m.as_str().parse().unwrap_or(1));
                let func_ctx = caps.get(5).map(|m| m.as_str().trim().to_string());

                current_hunk = Some(Hunk {
                    index: hunk_index,
                    anchor: String::new(), // computed after lines are added
                    header: line.to_string(),
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                    function_context: if func_ctx.as_ref().is_some_and(|s| !s.is_empty()) {
                        func_ctx
                    } else {
                        None
                    },
                });
            } else if let Some(ref mut hunk) = current_hunk {
                let diff_line = if let Some(rest) = line.strip_prefix('+') {
                    DiffLine::Add(rest.to_string())
                } else if let Some(rest) = line.strip_prefix('-') {
                    DiffLine::Remove(rest.to_string())
                } else if let Some(rest) = line.strip_prefix(' ') {
                    DiffLine::Context(rest.to_string())
                } else if line.starts_with('\\') {
                    // "\ No newline at end of file" - skip
                    continue;
                } else {
                    DiffLine::Context(line.to_string())
                };
                hunk.lines.push(diff_line);
            }
        }

        if let Some(mut h) = current_hunk {
            h.anchor = Hunk::compute_anchor(&h.content());
            hunks.push(h);
        }

        // Compute anchors for all hunks
        for hunk in &mut hunks {
            if hunk.anchor.is_empty() {
                hunk.anchor = Hunk::compute_anchor(&hunk.content());
            }
        }

        if !hunks.is_empty() {
            files.push(DiffFile {
                path,
                hunks,
                diff_header,
            });
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
index 1234567..abcdefg 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@ fn main() {
 fn main() {
-    println!("Hello");
+    println!("Hello, world!");
+    println!("Goodbye");
 }
"#;
        let files = parse_diff(diff).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(files[0].hunks.len(), 1);

        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.index, 1);
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 4);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 5);
        assert_eq!(hunk.additions(), 2);
        assert_eq!(hunk.deletions(), 1);
    }

    #[test]
    fn test_parse_multiple_hunks() {
        let diff = r#"diff --git a/file.txt b/file.txt
--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,3 @@
 line1
-old2
+new2
 line3
@@ -10,3 +10,4 @@
 line10
 line11
+inserted
 line12
"#;
        let files = parse_diff(diff).unwrap();
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].index, 1);
        assert_eq!(files[0].hunks[1].index, 2);
    }

    #[test]
    fn test_skip_binary() {
        let diff = r#"diff --git a/image.png b/image.png
Binary files a/image.png and b/image.png differ
"#;
        let files = parse_diff(diff).unwrap();
        assert!(files.is_empty());
    }

    fn replacement_block_hunk() -> Hunk {
        // Mirrors git's grouped output for replacing lines 2,3,4:
        //   a / -b -c -d / +B2 +C3 +D4 / e   (new lines a=1,B2=2,C3=3,D4=4,e=5)
        let mut hunk = Hunk {
            index: 1,
            anchor: String::new(),
            header: "@@ -1,5 +1,5 @@".to_string(),
            old_start: 1,
            old_count: 5,
            new_start: 1,
            new_count: 5,
            lines: vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Remove("b".to_string()),
                DiffLine::Remove("c".to_string()),
                DiffLine::Remove("d".to_string()),
                DiffLine::Add("B2".to_string()),
                DiffLine::Add("C3".to_string()),
                DiffLine::Add("D4".to_string()),
                DiffLine::Context("e".to_string()),
            ],
            function_context: None,
        };
        hunk.anchor = Hunk::compute_anchor(&hunk.content());
        hunk
    }

    #[test]
    fn test_restrict_replacement_middle() {
        // Stage only new-line 3 (c -> C3); b/d removals demote to context,
        // their additions drop. Pairing must keep the kept remove next to its
        // add despite git's grouped ordering.
        let trimmed = replacement_block_hunk()
            .restrict_to_lines(&[(3, 3)])
            .unwrap();
        assert_eq!(
            trimmed.lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Context("b".to_string()),
                DiffLine::Remove("c".to_string()),
                DiffLine::Add("C3".to_string()),
                DiffLine::Context("d".to_string()),
                DiffLine::Context("e".to_string()),
            ]
        );
        assert_eq!(trimmed.old_count, 5);
        assert_eq!(trimmed.new_count, 5);
        assert_eq!(trimmed.old_start, 1);
        assert_eq!(trimmed.new_start, 1);
    }

    #[test]
    fn test_restrict_replacement_range() {
        // Stage new-lines 2-3 (b->B2, c->C3); d->D4 stays unstaged.
        let trimmed = replacement_block_hunk()
            .restrict_to_lines(&[(2, 3)])
            .unwrap();
        assert_eq!(
            trimmed.lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Remove("b".to_string()),
                DiffLine::Add("B2".to_string()),
                DiffLine::Remove("c".to_string()),
                DiffLine::Add("C3".to_string()),
                DiffLine::Context("d".to_string()),
                DiffLine::Context("e".to_string()),
            ]
        );
        assert_eq!(trimmed.new_count, 5);
    }

    #[test]
    fn test_restrict_no_change_in_range() {
        // Range covers only unchanged context -> nothing to stage.
        assert!(
            replacement_block_hunk()
                .restrict_to_lines(&[(1, 1)])
                .is_none()
        );
    }

    #[test]
    fn test_restrict_insertion() {
        // a / +X / b : insertion sits at new-line 2.
        let mut hunk = Hunk {
            index: 1,
            anchor: String::new(),
            header: "@@ -1,2 +1,3 @@".to_string(),
            old_start: 1,
            old_count: 2,
            new_start: 1,
            new_count: 3,
            lines: vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Add("X".to_string()),
                DiffLine::Context("b".to_string()),
            ],
            function_context: None,
        };
        hunk.anchor = Hunk::compute_anchor(&hunk.content());

        let trimmed = hunk.restrict_to_lines(&[(2, 2)]).unwrap();
        assert_eq!(
            trimmed.lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Add("X".to_string()),
                DiffLine::Context("b".to_string()),
            ]
        );
        // Out-of-range insertion drops entirely.
        assert!(hunk.restrict_to_lines(&[(5, 5)]).is_none());
    }

    #[test]
    fn test_restrict_multiple_ranges() {
        // Stage new-lines 2 and 4 (b->B2, d->D4) but skip line 3 (c->C3).
        // Regression: a hunk spanning two disjoint requested ranges must keep
        // changes from *both*, not just the first matching range.
        let trimmed = replacement_block_hunk()
            .restrict_to_lines(&[(2, 2), (4, 4)])
            .unwrap();
        assert_eq!(
            trimmed.lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Remove("b".to_string()),
                DiffLine::Add("B2".to_string()),
                DiffLine::Context("c".to_string()),
                DiffLine::Remove("d".to_string()),
                DiffLine::Add("D4".to_string()),
                DiffLine::Context("e".to_string()),
            ]
        );
        assert_eq!(trimmed.new_count, 5);
    }

    #[test]
    fn test_restrict_pure_deletion_before_change() {
        // A standalone deletion before an in-range replacement, with a context
        // line separating the two runs (git's typical output). New-file lines:
        // a=1, c=2, D2=3, e=4 (b is deleted, occupies no new line). Staging
        // new-line 3 (d->D2) must NOT be thrown off by the earlier deletion of
        // b: a pure deletion consumes no new-file line, so the cursor stays put.
        //   a / -b / c / -d +D2 / e
        let mut hunk = Hunk {
            index: 1,
            anchor: String::new(),
            header: "@@ -1,5 +1,4 @@".to_string(),
            old_start: 1,
            old_count: 5,
            new_start: 1,
            new_count: 4,
            lines: vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Remove("b".to_string()),
                DiffLine::Context("c".to_string()),
                DiffLine::Remove("d".to_string()),
                DiffLine::Add("D2".to_string()),
                DiffLine::Context("e".to_string()),
            ],
            function_context: None,
        };
        hunk.anchor = Hunk::compute_anchor(&hunk.content());

        // Stage only new-line 3 (d->D2). The out-of-range `b` deletion drops
        // entirely (it has no new-file line); the in-range change survives.
        let trimmed = hunk.restrict_to_lines(&[(3, 3)]).unwrap();
        assert_eq!(
            trimmed.lines,
            vec![
                DiffLine::Context("a".to_string()),
                DiffLine::Context("c".to_string()),
                DiffLine::Remove("d".to_string()),
                DiffLine::Add("D2".to_string()),
                DiffLine::Context("e".to_string()),
            ]
        );

        // Stage only new-line 1 (the deletion's predecessor): nothing changes
        // there, and the in-range replacement at line 3 is left untouched.
        assert!(hunk.restrict_to_lines(&[(1, 1)]).is_none());
    }
}
