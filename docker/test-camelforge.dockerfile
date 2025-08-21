# Testing environment for CamelForge integration
FROM python:3.11-slim

WORKDIR /app

# Install dependencies
COPY requirements.txt .
RUN pip install -r requirements.txt

# Copy FraiseQL with CamelForge
COPY . .

# Install FraiseQL in development mode
RUN pip install -e .

# Create test script
RUN echo '#!/bin/bash\n\
echo "🧪 CamelForge Testing Environment"\n\
echo "================================"\n\
echo "Available commands:"\n\
echo "  test-basic    - Run basic functionality tests"\n\
echo "  test-perf     - Run performance comparison tests"\n\
echo "  test-safety   - Run safety and fallback tests"\n\
echo "  test-all      - Run complete test suite"\n\
echo ""\n\
echo "Environment variables:"\n\
echo "  FRAISEQL_CAMELFORGE_BETA=true/false"\n\
echo "  FRAISEQL_CAMELFORGE_DEBUG=true/false"\n\
echo "  FRAISEQL_CAMELFORGE_ALLOWLIST=entity1,entity2"\n\
echo ""\n\
exec "$@"' > /usr/local/bin/entrypoint.sh && chmod +x /usr/local/bin/entrypoint.sh

# Add test commands
RUN echo '#!/bin/bash\n\
export FRAISEQL_CAMELFORGE_BETA=true\n\
export FRAISEQL_CAMELFORGE_DEBUG=true\n\
python -m pytest tests/field_threshold/test_camelforge_integration.py -v' > /usr/local/bin/test-basic && chmod +x /usr/local/bin/test-basic

RUN echo '#!/bin/bash\n\
echo "Performance test: Standard vs CamelForge"\n\
export FRAISEQL_CAMELFORGE_COMPARE=true\n\
export FRAISEQL_CAMELFORGE_BETA=true\n\
python -m pytest tests/field_threshold/test_camelforge_complete_example.py::TestCamelForgeCompleteExample::test_performance_characteristics -v -s' > /usr/local/bin/test-perf && chmod +x /usr/local/bin/test-perf

RUN echo '#!/bin/bash\n\
echo "Safety test: Fallback behavior"\n\
export FRAISEQL_CAMELFORGE_BETA=true\n\
export FRAISEQL_CAMELFORGE_SAFE_MODE=true\n\
python -m pytest tests/field_threshold/ -v' > /usr/local/bin/test-safety && chmod +x /usr/local/bin/test-safety

RUN echo '#!/bin/bash\n\
echo "Complete test suite"\n\
export FRAISEQL_CAMELFORGE_BETA=true\n\
python -m pytest tests/field_threshold/ -v' > /usr/local/bin/test-all && chmod +x /usr/local/bin/test-all

ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
