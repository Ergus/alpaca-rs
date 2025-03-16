# Readme

Simple interface to Alpaca API in rust for rust.

The interface has 2 levels.

alpaca_client: A direct client to the alpaca API
alpaca_wrapper: It is a wrapper around the client in order to optimize multiple actions.

The wrapper performs optimization actions like caching, memoizations
and parallelization. The final application is intended to use the
wrapper and not the client directly.

This project is a cleaned version of
[DummyBot](https://github.com/Ergus/DummyBot) because I detected some
latency in DummyBot associated with Python. But also because I want to
use this from C an C++.
