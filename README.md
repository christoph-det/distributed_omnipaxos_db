# Implementation of Distributed SpacetimeDB with OmniPaxos

### Group members
- Senne Vanden Eynde
- Fredrik Ehne
- Christoph Dethloff

### Contribution
- Senne: 
    - code: implemented most of the node module and some tests
    - report: part of the unicache section, reflection and experience, and summary
    - bonus task: benchmark test and edited functions so that compressed message would work with existing implementation

- Christoph: 
    - code: bootstrapping and connections (in utils.rs), test helper functions and implementing most test cases, omnipaxos_durability - iteration of log and get_durable_offset some parts in node module regarding the message loop, replaying of transactions when leader is newly elected and bug fixing as well as revising test cases during testing
    - report: introduction, most of the test cases description, minor parts of design & implementation, future work, part of unicache section
    - bonus task: calculating message size, finding fix for unicache library, analyzing unicache details and deciding what to cache, dataset analyzation
- Fredrik:
    - code: implemented the omnipaxos_durability struct.
    - report: Most parts of design and implementation, some test cases and the illustrations.

### How to run

#### Executing the tests

To run tests individually, you can use the following command, replacing **INDIVIDUAL_TEST_NAME** with the name of the test you want to run:
```
cargo test --package omni-spacetimedb --bin omni-spacetimedb -- tests::tests::INDIVIDUAL_TEST_NAME --exact --nocapture
```
If you want to run all the tests at once, you can use 
```
cargo test --package omni-spacetimedb --bin omni-spacetimedb -- tests::tests --nocapture
```

Note that depending on how slow your machine works, you might have to change the timeouts used in the tests.

#### Executing Benchmarks for UniCache bonus task

To run the UniCache bonus task benchmark, please navigate to the *bonustask_no_datastore* branch.
