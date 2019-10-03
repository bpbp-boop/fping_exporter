# fping exporter

### About
A small rust wrapper around fping designed for monitoring subnets

### Prometheus Configuration

```
  - job_name: 'fping'
    metrics_path: /metrics
    static_configs:
      - targets:
        - 1.1.1.1/30
        - 8.8.8.8/32
    relabel_configs:
      - source_labels: [__address__]
        target_label: __param_target
      - source_labels: [__param_target]
        target_label: instance
      - target_label: __address__
        replacement: 127.0.0.1:9215  # The fping exporter's real hostname:port.
```

### Example metrics

```
# HELP ping_rtt_seconds Ping round trip time in seconds
# TYPE ping_rtt_seconds gauge
ping_rtt_seconds{address="1.1.1.1",sample="minimum"} 0.00055
ping_rtt_seconds{address="1.1.1.1",sample="average"} 0.00066
ping_rtt_seconds{address="1.1.1.1",sample="maxiumum"} 0.00095
ping_rtt_seconds{address="1.1.1.2",sample="minimum"} 0.00049
ping_rtt_seconds{address="1.1.1.2",sample="average"} 0.00068
ping_rtt_seconds{address="1.1.1.2",sample="maxiumum"} 0.00107


# HELP ping_packets_sent Ping packets sent
# TYPE ping_packets_sent gauge
ping_packets_sent{address="1.1.1.1"} 5
ping_packets_sent{address="1.1.1.2"} 5


# HELP ping_packets_received Ping packets received
# TYPE ping_packets_received gauge
ping_packets_received{address="1.1.1.1"} 5
ping_packets_received{address="1.1.1.2"} 5


# HELP ping_packet_loss_percent Percent of ping packets lost
# TYPE ping_packet_loss_percent gauge
ping_packet_loss_percent{address="1.1.1.1"} 0
ping_packet_loss_percent{address="1.1.1.2"} 0
```