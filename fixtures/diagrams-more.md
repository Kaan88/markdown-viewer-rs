# More Mermaid diagrams

One sample of each family beyond the eight in `diagrams.md`. Every one draws as an image; the last section shows a family that stays disabled and falls back to its source.

## Timeline

```mermaid
timeline
    title History of Social Media Platform
    2002 : LinkedIn
    2004 : Facebook : Google
    2005 : YouTube
    2006 : Twitter
```

## User journey

```mermaid
journey
    title My working day
    section Go to work
      Make tea: 5: Me
      Go upstairs: 3: Me
      Do work: 1: Me, Cat
    section Go home
      Go downstairs: 5: Me
      Sit down: 5: Me
```

## Git graph

```mermaid
gitGraph
    commit
    commit
    branch develop
    checkout develop
    commit
    commit
    checkout main
    merge develop
    commit
    branch feature
    checkout feature
    commit id: "wip"
    checkout main
    merge feature tag: "v1.0"
```

## C4 context

```mermaid
C4Context
    title System Context diagram for Internet Banking System
    Person(customerA, "Banking Customer A", "A customer of the bank, with personal bank accounts.")
    System(SystemAA, "Internet Banking System", "Allows customers to view information about their bank accounts.")
    System_Ext(SystemE, "E-mail system", "The internal Microsoft Exchange e-mail system.")
    System_Ext(SystemC, "Mainframe Banking System", "Stores all of the core banking information.")
    Rel(customerA, SystemAA, "Uses")
    Rel(SystemAA, SystemE, "Sends e-mails", "SMTP")
    Rel(SystemAA, SystemC, "Gets account information from")
```

## Sankey

```mermaid
sankey-beta

Agricultural 'waste',Bio-conversion,124.729
Bio-conversion,Liquid,0.597
Bio-conversion,Solid,26.862
Bio-conversion,Gas,280.322
Biofuel imports,Liquid,35
Coal imports,Coal,11.606
Coal,Solid,75.571
```

## XY chart

```mermaid
xychart-beta
    title "Sales Revenue"
    x-axis [jan, feb, mar, apr, may, jun, jul, aug, sep, oct, nov, dec]
    y-axis "Revenue (in $)" 4000 --> 11000
    bar [5000, 6000, 7500, 8200, 9500, 10500, 11000, 10200, 9200, 8500, 7000, 6000]
    line [5000, 6000, 7500, 8200, 9500, 10500, 11000, 10200, 9200, 8500, 7000, 6000]
```

## Quadrant chart

```mermaid
quadrantChart
    title Reach and engagement of campaigns
    x-axis Low Reach --> High Reach
    y-axis Low Engagement --> High Engagement
    quadrant-1 We should expand
    quadrant-2 Need to promote
    quadrant-3 Re-evaluate
    quadrant-4 May be improved
    Campaign A: [0.3, 0.6]
    Campaign B: [0.45, 0.23]
    Campaign C: [0.57, 0.69]
    Campaign D: [0.78, 0.34]
    Campaign E: [0.40, 0.34]
    Campaign F: [0.35, 0.78]
```

## Requirement

```mermaid
requirementDiagram
    requirement test_req {
    id: 1
    text: the test text.
    risk: high
    verifymethod: test
    }
    functionalRequirement test_req2 {
    id: 1.1
    text: the second test text.
    risk: low
    verifymethod: inspection
    }
    element test_entity {
    type: simulation
    }
    test_entity - satisfies -> test_req2
    test_req - contains -> test_req2
```

## Block

```mermaid
block-beta
    columns 3
    a["Frontend"] b["API"] c["Database"]
    d["Cache"]:2 e["Queue"]
    a --> b
    b --> c
    b --> d
```

## Packet

```mermaid
packet-beta
0-15: "Source Port"
16-31: "Destination Port"
32-63: "Sequence Number"
64-95: "Acknowledgment Number"
96-99: "Data Offset"
100-105: "Reserved"
106: "URG"
107: "ACK"
108-111: "Flags"
112-127: "Window"
```

## Kanban

```mermaid
kanban
  Todo
    [Create Documentation]
    docs[Create Blog about the new diagram]
  [In progress]
    id6[Create renderer so that it works in all cases.]
  id9[Ready for deploy]
    id8[Design grammar]@{ assigned: 'knsv' }
  id10[Done]
    id5[define getData]
```

## Radar

```mermaid
radar-beta
  title Skills
  axis a["Design"], b["Code"], c["Tests"], d["Docs"], e["Ops"]
  curve one["Alice"]{4, 5, 3, 2, 3}
  curve two["Bob"]{2, 3, 5, 4, 4}
  max 5
```

## Treemap

```mermaid
treemap-beta
"Section 1"
    "Leaf 1.1": 12
    "Section 1.2"
      "Leaf 1.2.1": 12
"Section 2"
    "Leaf 2.1": 20
    "Leaf 2.2": 25
```

## Ishikawa (fishbone)

```mermaid
ishikawa-beta
    Blurry Photo
    Process
        Out of focus
        Shutter speed too slow
    User
        Shaky hands
        Poor framing
    Equipment
        Dirty lens
        Low battery
```

## Tree view

```mermaid
treeView-beta
    "packages"
        "mdv-core"
            "src"
            "Cargo.toml"
        "mdv-render"
            "src"
```

## Venn

```mermaid
venn-beta
  title Team overlap
  set A["Frontend"]
  set B["Backend"]
  set C["Design"]
  union A,B["Fullstack"]
  union A,C["Product UI"]
```

## Use case

```mermaid
usecase-beta
actor Customer
actor Clerk
Customer --> Browse
Customer --> Checkout
Clerk --> Checkout
Clerk --> Refund
```

## ZenUML

```mermaid
zenuml
    title Order flow
    Client->Server: placeOrder
    Server->Database: save
    Database->Server: ok
    Server->Client: confirmation
```

## Cynefin

```mermaid
cynefin-beta
  title Incident
  complex
    "Experiment"
  complicated
    "Analyse"
  clear
    "Best practice"
  chaotic
    "Act first"
```

## Event modeling

```mermaid
eventmodeling
tf 01 ui CartScreen
tf 02 cmd AddItem
tf 03 evt ItemAdded
tf 04 rmo CartItems
```

## Agent flow

```mermaid
agentflow-beta
Planner[Planner] --> Tool[Run tool]
Tool --> Check{Done?}
Check --> Planner
Check --> Answer[Answer]
```

## Swimlane

```mermaid
swimlane-beta
subgraph Customer
  A[Order] --> B[Pay]
end
subgraph Shop
  C[Pack] --> D[Ship]
end
B --> C
```

## Railroad (EBNF)

```mermaid
railroad-ebnf-beta
number = num , { num } ;
sum = number , { ( "+" | "-" ) , number } ;
group = "(" , sum , ")" ;
```

## Not enabled: architecture

Architecture diagrams need merman's Cytoscape layout, which is left out, so this block keeps its labelled source.

```mermaid
architecture-beta
    group api(cloud)[API]

    service db(database)[Database] in api
    service disk1(disk)[Storage] in api
    service server(server)[Server] in api

    db:L -- R:server
    disk1:T -- B:server
```
