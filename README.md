# fping exporter

### About
Prometheus exporter to run fping against multiple subnets. Tested again 150+ /24s
Each subnet specified will be run in a background thread
* each thread is offset from starting by a random number of seconds to avoid flooding the network
* each subnet will be pinged once every 60 seconds


note: adjust kernel limits if needed
```
file: /proc/sys/net/ipv4/icmp_msgs_per_sec (default 1000)
variable: net.ipv4.icmp_msgs_per_sec

file: /proc/sys/net/ipv4/icmp_msgs_burst (default 50)
variable: net.ipv4.icmp_msgs_burst
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

# HELP ping_packet_loss_percent Percent of ping packets lost
# TYPE ping_packet_loss_percent gauge
ping_packet_loss_percent{address="1.1.1.1"} 0
ping_packet_loss_percent{address="1.1.1.2"} 0
```